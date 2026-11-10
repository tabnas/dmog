//! The streaming gate: every format the formats table lists is held to the
//! definition of streaming in tabnas/alchemy's `docs/streaming-plan.md`,
//! section 13 (admin ADR-33):
//!
//! > A format streams, for the gate, when in every runtime a document of it
//! > read from a file and from standard input has written its first byte
//! > before its input ends, and the run's peak memory stays flat over ten
//! > times the document.
//!
//! The documents are `test/streaming.json`'s, which the TypeScript and Go
//! gates (`ts/test/streaming.test.ts`, `go/cmd/dmog/streaming_test.go`) read
//! too: for each format, a document of `items` items and one of ten times as
//! many. Each format is held to two clauses:
//!
//! - **written before the end**: `dmog translate --from F --to json` is
//!   handed the document of `items` items in two parts, from standard input
//!   and from a named pipe given as INPUT (a FIFO, which `mkfifo` makes;
//!   there is none on Windows, where only standard input is run), and must
//!   write to standard output after the first part, which holds nine tenths
//!   of the items, before the rest is sent; the run must then end as a run
//!   over the whole document does, with the same output;
//! - **flat memory**: the peak of what the run holds, over the document of
//!   `items` items and over ten times as many, must stay within
//!   [`BOUND`] of each other.
//!
//! A format that fails either clause is registered in [`NOT_STREAMING`]
//! with the clauses it fails and why; the gate holds each entry to failing
//! exactly those, so a format that starts streaming fails it until its entry
//! is deleted. The formats the maintainer ruled whole are listed apart
//! ([`RULED_WHOLE`]) and are not run.
//!
//! The memory is measured in process, over `tabnas_dmog::translate`'s
//! `run_document`, the call the binary makes for standard input: a counting
//! global allocator keeps, for each thread, the bytes it holds and the most
//! it has held since its measure began, and a run of `translate` is
//! synchronous on the thread that calls it. The document is made as it is
//! read ([`Made`]) and the output is kept as its length and a hash
//! ([`Written`]), so the test holds neither, and the count is the run's
//! alone: exact, in bytes, the same in a debug and a release build, and free
//! of what an allocator or the operating system keeps back, which a peak
//! resident set (`VmHWM`) would carry. Clause 1 runs the binary itself.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tabnas_alchemy::translate::Options;
use tabnas_dmog::translate::{self, Document, Format, Request};
use tabnas_transduce::{Limits, Metrics};

// ---------------------------------------------------------------------
// The ledgers
// ---------------------------------------------------------------------

/// The clauses of the definition a format fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fails {
    /// It writes nothing before its input ends, from standard input or
    /// from a FIFO.
    Written,
    /// Its peak memory over ten times the document is past [`BOUND`] of
    /// its peak over the document.
    Flat,
    /// Both.
    Both,
}

/// The formats the maintainer ruled whole (admin ADR-33, ruling 4: "expr
/// and semver stay whole"): their documents are read whole by design, as
/// their `whole` sentence in the formats table says, and the gate does not
/// run them.
const RULED_WHOLE: [(&str, &str); 2] = [
    (
        "expr",
        "an expression's tree is complete only at its end; admin ADR-33, ruling 4",
    ),
    (
        "semver",
        "a version is made only once its whole text is accepted; admin ADR-33, ruling 4",
    ),
];

/// The formats that do not stream yet in this runtime, each with the
/// clauses it fails and why. The gate holds each entry to failing exactly
/// those clauses: delete an entry, or a clause of it, once its format
/// passes.
const NOT_STREAMING: [(&str, Fails, &str); 10] = [
    (
        "abnf",
        Fails::Both,
        "it has no transducer part: a document is read whole as the grammar spec its compiler \
         writes, until abnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
    ),
    (
        "c",
        Fails::Both,
        "it has no transducer part: a translation unit is read whole as the tree c's parse \
         realizes (admin ADR-33, ruling 7; the streaming plan's step 21, whose ruling is pending)",
    ),
    (
        "css",
        Fails::Flat,
        "css is not in transduce's prune suite, so the engine's tree keeps every rule \
         (tabnas/transduce#82)",
    ),
    (
        "ebnf",
        Fails::Both,
        "it has no transducer part: a document is read whole as the grammar spec its compiler \
         writes, until ebnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
    ),
    (
        "gbnf",
        Fails::Both,
        "it has no transducer part: a document is read whole as the grammar spec its compiler \
         writes, until gbnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
    ),
    (
        "ini",
        Fails::Flat,
        "every section is a member of the document's root map, and pruning empties no map, so \
         the engine's tree keeps every section (tabnas/transduce#81)",
    ),
    (
        "jsonic",
        Fails::Both,
        "jsonic's constructor declares no lex.window, so the engine reads a document whole \
         before its parse begins (tabnas/jsonic#121)",
    ),
    (
        "markdown",
        Fails::Flat,
        "a document's blocks array is no rule's node, so pruning never empties it, and each \
         block's node stays (tabnas/transduce#81)",
    ),
    (
        "proto",
        Fails::Both,
        "the check of its nesting that proto's parse makes first counts the whole document, \
         and proto's grammar declares no lex.window, so a document is read whole first \
         (tabnas/proto#81)",
    ),
    (
        "toml",
        Fails::Flat,
        "an array of tables is no rule's node, so pruning never empties it, and each table \
         stays whole (tabnas/transduce#81)",
    ),
];

/// How far the peak over ten times the document may pass the peak over the
/// document: 1.25 times it. Measured with `test/streaming.json`'s documents
/// (300 items, about 315 KB, and 3,000), the formats that stream hold the
/// same within a twentieth either way, where in a window's cycle the peak
/// falls: json 1.67 MB and 1.66 MB, csv 1.74 MB and 1.74 MB, yaml 2.24 MB
/// and 2.24 MB, json5 1.86 MB and 1.78 MB, xml 1.72 MB and 1.72 MB. A format
/// that holds something of its document grows with it, run to its end:
/// jsonic, read whole, 2.35 MB and 19.31 MB (8.2 times), css 2.76 MB and
/// 10.10 MB, ini, whose sections stay in the engine's tree, 2.36 MB and
/// 6.64 MB (2.8 times), and the least, markdown, which keeps a node of every
/// block, 2.13 MB and 3.61 MB (1.7 times). JSON read whole (`--whole`) holds
/// 2.60 MB and 23.48 MB (9 times), which
/// [`a_document_held_whole_fails_the_bound`] holds to fail it.
const BOUND: f64 = 1.25;

/// The entry of `id`, if it has one.
fn registered(id: &str) -> Option<(Fails, &'static str)> {
    NOT_STREAMING
        .iter()
        .find(|(entry, _, _)| *entry == id)
        .map(|(_, fails, why)| (*fails, *why))
}

fn ruled_whole(id: &str) -> bool {
    RULED_WHOLE.iter().any(|(whole, _)| *whole == id)
}

// ---------------------------------------------------------------------
// What a thread holds
// ---------------------------------------------------------------------

/// The allocator every allocation of this test binary goes through: the
/// system's, counting for each thread the bytes it holds and the most it
/// has held since its measure began ([`measure_from_here`]). The counts are
/// thread-local cells, which allocate nothing. Memory one thread allocates
/// and another frees is counted on each where it happened, which a run of
/// `translate`, synchronous on its thread, does not do.
struct Counting;

thread_local! {
    static HELD: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn count(delta: isize) {
    let _ = HELD.try_with(|held| {
        let now = held.get() + delta;
        held.set(now);
        if delta > 0 {
            let _ = PEAK.try_with(|peak| {
                if now > peak.get() {
                    peak.set(now)
                }
            });
        }
    });
}

// SAFETY: every call is the system allocator's, with the caller's
// arguments, and its answer is returned unchanged; the counting only reads
// the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            count(layout.size() as isize);
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            count(layout.size() as isize);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(block, layout) };
        count(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        let moved = unsafe { System.realloc(block, layout, size) };
        if !moved.is_null() {
            count(size as isize - layout.size() as isize);
        }
        moved
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Begin a measure on this thread: what it holds now is the zero its peak
/// is counted from.
fn measure_from_here() {
    HELD.with(|held| held.set(0));
    PEAK.with(|peak| peak.set(0));
}

/// The most this thread has held since its measure began.
fn peak() -> usize {
    PEAK.with(|peak| peak.get().max(0) as usize)
}

// ---------------------------------------------------------------------
// The documents
// ---------------------------------------------------------------------

/// A format's document in `test/streaming.json`: `head`, then `item` once
/// for each index with `{i}` the index, `separator` between two items, and
/// `tail`; `{text}` in any of them is the file's `text`.
#[derive(Clone)]
struct Template {
    head: String,
    item: String,
    separator: String,
    tail: String,
}

impl Template {
    fn item(&self, i: usize) -> String {
        self.item.replace("{i}", &i.to_string())
    }

    /// The document of `n` items in two parts: the head and the first
    /// `split` items, and the rest.
    fn parts(&self, n: usize, split: usize) -> (Vec<u8>, Vec<u8>) {
        let mut first = self.head.clone();
        let mut rest = String::new();
        for i in 0..n {
            let into = if i < split { &mut first } else { &mut rest };
            if i > 0 {
                into.push_str(&self.separator);
            }
            into.push_str(&self.item(i));
        }
        rest.push_str(&self.tail);
        (first.into_bytes(), rest.into_bytes())
    }
}

/// `test/streaming.json`: the number of items, and each format's document.
struct Documents {
    items: usize,
    templates: BTreeMap<String, Template>,
}

fn documents() -> &'static Documents {
    static DOCUMENTS: std::sync::OnceLock<Documents> = std::sync::OnceLock::new();
    DOCUMENTS.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test/streaming.json");
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file: serde_json::Value = serde_json::from_str(&text).expect("test/streaming.json");
        let pad = file["text"].as_str().expect("the text");
        let items = file["items"].as_u64().expect("the items") as usize;
        let templates = file["documents"]
            .as_object()
            .expect("the documents")
            .iter()
            .map(|(id, t)| {
                let field = |k: &str| {
                    t[k].as_str()
                        .unwrap_or_else(|| panic!("{id}'s {k}"))
                        .replace("{text}", pad)
                };
                let template = Template {
                    head: field("head"),
                    item: field("item"),
                    separator: field("separator"),
                    tail: field("tail"),
                };
                (id.clone(), template)
            })
            .collect();
        Documents { items, templates }
    })
}

/// The answer a reader or a writer gives once the run has held more than
/// its measure allows: the run stops there, past the bound already.
const OVER: &str = "the streaming gate stopped the run: it held more than the bound";

/// A document of `n` items, made as it is read, so that the test holds one
/// item of it at a time. With a limit, a read once this thread has held
/// more than it fails, and stops the run.
struct Made {
    template: Template,
    n: usize,
    next: usize,
    pending: Vec<u8>,
    at: usize,
    tail_sent: bool,
    limit: Option<usize>,
    over: Arc<AtomicBool>,
}

impl Made {
    fn new(template: Template, n: usize, limit: Option<usize>, over: Arc<AtomicBool>) -> Made {
        let pending = template.head.clone().into_bytes();
        Made {
            template,
            n,
            next: 0,
            pending,
            at: 0,
            tail_sent: false,
            limit,
            over,
        }
    }
}

impl Read for Made {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.limit.is_some_and(|limit| peak() > limit) {
            self.over.store(true, Ordering::SeqCst);
            return Err(io::Error::other(OVER));
        }
        let mut written = 0;
        while written < buf.len() {
            if self.at == self.pending.len() {
                self.pending.clear();
                self.at = 0;
                if self.next < self.n {
                    if self.next > 0 {
                        self.pending
                            .extend_from_slice(self.template.separator.as_bytes());
                    }
                    let item = self.template.item(self.next);
                    self.pending.extend_from_slice(item.as_bytes());
                    self.next += 1;
                } else if !self.tail_sent {
                    self.pending
                        .extend_from_slice(self.template.tail.as_bytes());
                    self.tail_sent = true;
                } else {
                    break;
                }
                continue;
            }
            let k = (self.pending.len() - self.at).min(buf.len() - written);
            buf[written..written + k].copy_from_slice(&self.pending[self.at..self.at + k]);
            self.at += k;
            written += k;
        }
        Ok(written)
    }
}

/// FNV-1a over bytes, from `hash`: a run's output kept as its length and
/// this hash, never its bytes.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// What a run writes, as its length and its hash. With a limit, a write
/// once this thread has held more than it fails, and stops the run.
#[derive(Clone)]
struct Written {
    seen: Arc<Mutex<(u64, u64)>>,
    limit: Option<usize>,
    over: Arc<AtomicBool>,
}

impl Write for Written {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.limit.is_some_and(|limit| peak() > limit) {
            self.over.store(true, Ordering::SeqCst);
            return Err(io::Error::other(OVER));
        }
        let mut seen = self.seen.lock().expect("the output's count");
        seen.0 += buf.len() as u64;
        seen.1 = fnv(seen.1, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Clause 2: the run's peak
// ---------------------------------------------------------------------

/// One run in process.
struct Run {
    /// The most the run held, in bytes, past what its thread held when it
    /// began; with `over`, the most it had held when it was stopped.
    peak: usize,
    /// It was stopped past its limit.
    over: bool,
    /// The output's length and hash.
    output: (u64, u64),
    took: Duration,
}

fn request(from: &'static Format, whole: bool) -> Request<'static> {
    Request {
        from,
        to: translate::format("json").expect("json is a format"),
        path: None,
        options: Options::default(),
        program: None,
        whole,
        limits: Limits::default(),
    }
}

/// `translate --from F --to json` over a document of `n` items, read as
/// standard input is, a reader the run pulls from, in process on this
/// thread. The composition is compiled first, outside the measure.
fn measure(from: &'static Format, n: usize, whole: bool, limit: Option<usize>) -> Run {
    let id = from.id();
    let template = documents().templates[id].clone();
    let request = request(from, whole);
    let compiled = translate::compile(&request).unwrap_or_else(|f| panic!("{id}: {f}"));
    let over = Arc::new(AtomicBool::new(false));
    let written = Written {
        seen: Arc::new(Mutex::new((0, FNV_OFFSET))),
        limit,
        over: over.clone(),
    };
    let document = Document::Reader(Box::new(Made::new(template, n, limit, over.clone())));
    let out: Box<dyn Write + Send> = Box::new(written.clone());
    let metrics = Metrics::new();
    let started = Instant::now();
    measure_from_here();
    let outcome = translate::run_document(&request, &compiled, document, out, metrics);
    let peak = peak();
    let took = started.elapsed();
    let over = over.load(Ordering::SeqCst);
    if let Err(fail) = outcome {
        assert!(over, "{id}, {n} items: {fail}");
    }
    let output = *written.seen.lock().expect("the output's count");
    Run {
        peak,
        over,
        output,
        took,
    }
}

/// The most a run over ten times the document may hold.
fn bound(peak: usize) -> usize {
    (peak as f64 * BOUND) as usize
}

fn mb(bytes: usize) -> String {
    format!("{:.2} MB", bytes as f64 / 1_048_576.0)
}

// ---------------------------------------------------------------------
// Clause 1: written before the end
// ---------------------------------------------------------------------

/// How a document reaches the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// Standard input, a pipe: INPUT is `-`.
    Stdin,
    /// A named pipe, given as INPUT, which the command opens as a file.
    Fifo,
}

/// The routes this platform has: a FIFO is made with `mkfifo`, which is
/// POSIX, so Windows runs standard input alone.
fn routes() -> &'static [Route] {
    if cfg!(unix) {
        &[Route::Stdin, Route::Fifo]
    } else {
        &[Route::Stdin]
    }
}

/// What one run of the binary over a document handed over in two parts
/// did.
struct Fed {
    /// It wrote after the first part and before the rest was sent.
    before_the_end: bool,
    /// How long after it started it first wrote, or how long the test
    /// waited for that.
    waited: Duration,
    status: Option<i32>,
    output: (u64, u64),
    stderr: String,
}

/// Run `dmog translate --from id --to json` over a document handed over in
/// two parts by `route`: `first`, then, once the command has written or
/// `wait` has passed, `rest`, and the end of the input. Standard output and
/// standard error are read throughout, on threads of their own, so that a
/// command that writes as it reads never waits on a pipe nobody reads.
fn feed(id: &str, route: Route, first: Vec<u8>, rest: Vec<u8>, wait: Duration, dir: &Path) -> Fed {
    let fifo = dir.join(format!("{id}.fifo"));
    let input = match route {
        Route::Stdin => "-".to_string(),
        Route::Fifo => {
            let made = Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .expect("mkfifo runs");
            assert!(made.success(), "mkfifo {}", fifo.display());
            fifo.display().to_string()
        }
    };
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dmog"))
        .args(["translate", "--from", id, "--to", "json", &input])
        .stdin(match route {
            Route::Stdin => Stdio::piped(),
            Route::Fifo => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the dmog binary starts");
    let (wrote, first_byte) = mpsc::channel::<()>();
    let mut stdout = child.stdout.take().expect("a piped stdout");
    let reading = thread::spawn(move || {
        let (mut length, mut hash, mut told) = (0u64, FNV_OFFSET, false);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(k) => {
                    length += k as u64;
                    hash = fnv(hash, &buf[..k]);
                    if !told {
                        let _ = wrote.send(());
                        told = true;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        (length, hash)
    });
    let mut stderr = child.stderr.take().expect("a piped stderr");
    let errors = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let (go, go_on) = mpsc::channel::<()>();
    let opened = Arc::new(AtomicBool::new(false));
    let stdin = child.stdin.take();
    let writing = {
        let (fifo, opened) = (fifo.clone(), opened.clone());
        thread::spawn(move || -> io::Result<()> {
            // A FIFO's open waits for the command to open it to read.
            let mut sink: Box<dyn Write> = match stdin {
                Some(pipe) => Box::new(pipe),
                None => Box::new(std::fs::OpenOptions::new().write(true).open(&fifo)?),
            };
            opened.store(true, Ordering::SeqCst);
            sink.write_all(&first)?;
            sink.flush()?;
            let _ = go_on.recv();
            sink.write_all(&rest)?;
            sink.flush()
        })
    };
    let before_the_end = first_byte.recv_timeout(wait).is_ok();
    let waited = started.elapsed();
    let _ = go.send(());
    // The run has its time again to finish, and a generous margin.
    let deadline = Instant::now() + Duration::from_secs(300) + wait * 4;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    // A command that ended before it opened the FIFO leaves the writer
    // waiting in its open: opening the FIFO to read lets it through, and
    // its write then fails, as one to a command that has ended does.
    if route == Route::Fifo && !opened.load(Ordering::SeqCst) {
        let _ = std::fs::File::open(&fifo);
    }
    let _ = writing.join();
    let output = reading.join().expect("standard output is read");
    let stderr = errors.join().expect("standard error is read");
    if route == Route::Fifo {
        let _ = std::fs::remove_file(&fifo);
    }
    Fed {
        before_the_end,
        waited,
        status,
        output,
        stderr,
    }
}

// ---------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------

/// What the gate found of one format.
struct Found {
    id: String,
    /// Each route, and when the command first wrote, where it wrote before
    /// the input ended.
    written: Vec<(Route, Option<Duration>)>,
    /// The peak over the document, and over ten times the document.
    peaks: (Run, Run),
    /// What went wrong that no ledger excuses: a run that failed, or ended
    /// otherwise than the run over the whole document.
    broken: Vec<String>,
}

impl Found {
    fn writes_before_the_end(&self) -> bool {
        self.written.iter().all(|(_, when)| when.is_some())
    }

    fn flat(&self) -> bool {
        let (one, ten) = &self.peaks;
        !ten.over && ten.peak <= bound(one.peak)
    }

    fn fails(&self) -> Option<Fails> {
        match (self.writes_before_the_end(), self.flat()) {
            (true, true) => None,
            (false, true) => Some(Fails::Written),
            (true, false) => Some(Fails::Flat),
            (false, false) => Some(Fails::Both),
        }
    }

    /// The measurements, in a line.
    fn summary(&self) -> String {
        let written: Vec<String> = self
            .written
            .iter()
            .map(|(route, when)| {
                let route = match route {
                    Route::Stdin => "standard input",
                    Route::Fifo => "a FIFO",
                };
                match when {
                    Some(when) => format!("from {route} written after {:.1} s", when.as_secs_f64()),
                    None => format!("from {route} nothing written before the end"),
                }
            })
            .collect();
        let (one, ten) = &self.peaks;
        let n = documents().items;
        format!(
            "{}; peak {} at {n} items and {} at {} (bound {}{}), {:.1} s and {:.1} s",
            written.join(", "),
            mb(one.peak),
            mb(ten.peak),
            n * 10,
            mb(bound(one.peak)),
            if ten.over { ", stopped past it" } else { "" },
            one.took.as_secs_f64(),
            ten.took.as_secs_f64(),
        )
    }
}

/// Run `work` on a thread of the stack the binary runs its command on
/// (`tabnas_alchemy::STACK_BYTES`), so a debug build's evaluator has the
/// room a release build's has.
fn on_a_command_stack<'scope, T: Send + 'scope>(
    scope: &'scope thread::Scope<'scope, '_>,
    work: impl FnOnce() -> T + Send + 'scope,
) -> thread::ScopedJoinHandle<'scope, T> {
    thread::Builder::new()
        .stack_size(tabnas_alchemy::STACK_BYTES)
        .spawn_scoped(scope, work)
        .expect("a thread starts")
}

/// Hold one format to both clauses.
fn hold(from: &'static Format, dir: &Path) -> Found {
    let id = from.id().to_string();
    let n = documents().items;
    let expected = registered(&id).map(|(fails, _)| fails);
    let one = measure(from, n, false, None);
    let mut broken = Vec::new();
    // A command expected to write before the end is given a generous time
    // to; one registered as not writing is given twice the time a whole run
    // took in process, and two seconds to start and compile, which a run
    // that wrote as it read would not need.
    let expect_writes = !matches!(expected, Some(Fails::Written | Fails::Both));
    let wait = if expect_writes {
        Duration::from_secs(60).max(one.took * 10)
    } else {
        one.took * 2 + Duration::from_secs(2)
    };
    let (first, rest) = documents().templates[&id].parts(n, n * 9 / 10);
    let feeds: Vec<(Route, Fed)> = thread::scope(|scope| {
        let runs: Vec<_> = routes()
            .iter()
            .map(|route| {
                let (first, rest, id) = (first.clone(), rest.clone(), id.clone());
                (
                    *route,
                    scope.spawn(move || feed(&id, *route, first, rest, wait, dir)),
                )
            })
            .collect();
        runs.into_iter()
            .map(|(route, run)| (route, run.join().expect("the run is fed")))
            .collect()
    });
    let mut written = Vec::new();
    for (route, fed) in feeds {
        if fed.status != Some(0) {
            broken.push(format!(
                "{id} from {route:?}: status {:?}: {}",
                fed.status,
                fed.stderr.trim()
            ));
        } else if fed.output != one.output {
            broken.push(format!(
                "{id} from {route:?}: {} bytes written, where the run over the whole document \
                 wrote {}, or other bytes",
                fed.output.0, one.output.0
            ));
        }
        written.push((route, fed.before_the_end.then_some(fed.waited)));
    }
    let ten = measure(from, n * 10, false, Some(bound(one.peak)));
    Found {
        id,
        written,
        peaks: (one, ten),
        broken,
    }
}

/// Every format the formats table lists streams in this runtime, or is
/// registered: in [`NOT_STREAMING`], failing exactly the clauses its entry
/// names, or in [`RULED_WHOLE`]. A few formats are held at once, each on a
/// thread of its own, whose count of what it holds is its own.
#[test]
fn every_format_streams_or_is_registered() {
    let dir = std::env::temp_dir().join(format!("dmog-streaming-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let held: Vec<&'static Format> = translate::formats()
        .iter()
        .filter(|f| !ruled_whole(f.id()))
        .collect();
    for format in &held {
        assert!(
            documents().templates.contains_key(format.id()),
            "{} is a format with no document in test/streaming.json",
            format.id()
        );
    }
    let total = held.len();
    // In the table's order, so that the formats whose runs take longest
    // (csv, read slowest, and those read whole) start among the first.
    let queue = Mutex::new(std::collections::VecDeque::from(held));
    let done = AtomicUsize::new(0);
    let found = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 3));
    thread::scope(|scope| {
        for _ in 0..workers {
            on_a_command_stack(scope, || loop {
                let Some(format) = queue.lock().expect("the queue").pop_front() else {
                    break;
                };
                let one = hold(format, &dir);
                let k = done.fetch_add(1, Ordering::SeqCst) + 1;
                println!(
                    "streaming gate: {k} of {total} ({}%) {}: {}",
                    k * 100 / total,
                    one.id,
                    one.summary()
                );
                found.lock().expect("the findings").push(one);
            });
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
    let mut wrong = Vec::new();
    for one in found.into_inner().expect("the findings") {
        wrong.extend(one.broken.iter().cloned());
        let expected = registered(&one.id);
        match (one.fails(), expected) {
            (None, None) => {}
            (Some(fails), Some((entry, _))) if fails == entry => {}
            (None, Some((entry, why))) => wrong.push(format!(
                "{} streams now ({}): delete its entry, {entry:?} ({why})",
                one.id,
                one.summary()
            )),
            (Some(fails), None) => wrong.push(format!(
                "{} does not stream: it fails {fails:?} ({}); register it with why",
                one.id,
                one.summary()
            )),
            (Some(fails), Some((entry, why))) => wrong.push(format!(
                "{} fails {fails:?}, where its entry says {entry:?} ({}): correct the entry \
                 ({why})",
                one.id,
                one.summary()
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "the streaming gate:\n{}",
        wrong.join("\n")
    );
}

/// The bound holds a document read whole to fail: JSON read with
/// `--whole`, its grammar's value walked, holds more over ten times the
/// document than the bound lets a run hold.
#[test]
fn a_document_held_whole_fails_the_bound() {
    let json = translate::format("json").expect("json is a format");
    let n = documents().items;
    let (one, ten) = thread::scope(|scope| {
        on_a_command_stack(scope, || {
            let one = measure(json, n, true, None);
            let ten = measure(json, n * 10, true, Some(bound(one.peak)));
            (one, ten)
        })
        .join()
        .expect("json is read whole")
    });
    println!(
        "json read whole: peak {} at {n} items, and past {} at {}: {}",
        mb(one.peak),
        mb(ten.peak),
        n * 10,
        if ten.over {
            "stopped past the bound"
        } else {
            "ran to its end"
        }
    );
    assert!(
        ten.over || ten.peak > bound(one.peak),
        "json read whole held {} at {} items and {} at {}, within the bound {}",
        mb(one.peak),
        n,
        mb(ten.peak),
        n * 10,
        mb(bound(one.peak))
    );
}

/// The ledgers and the documents name the formats of the table: each entry
/// a format that is run, each ruling a format the table says is read whole,
/// and each document a format the gate runs.
#[test]
fn the_ledgers_and_the_documents_are_the_tables() {
    for (id, _, why) in NOT_STREAMING {
        let format = translate::format(id).unwrap_or_else(|| panic!("{id} ({why}) is no format"));
        assert!(
            !ruled_whole(format.id()),
            "{id} is registered and ruled whole"
        );
    }
    for (id, why) in RULED_WHOLE {
        let format = translate::format(id).unwrap_or_else(|| panic!("{id} ({why}) is no format"));
        assert!(
            format.part.whole.is_some() && format.transducer().is_none(),
            "{id} is ruled whole ({why}), but the formats table no longer reads it whole"
        );
    }
    for id in documents().templates.keys() {
        assert!(
            translate::format(id).is_some_and(|f| !ruled_whole(f.id())),
            "test/streaming.json has a document for {id}, which the gate does not run"
        );
    }
}
