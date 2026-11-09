//! YAML's render against YAML's own reader: the round trip the translation
//! design asks of it (transduce's `docs/translation.md`, step 3). Every
//! YAML input the sibling tabnas/yaml checkout's fixtures hold, read,
//! written through `translate` as YAML, and read back, is the same value.
//! It lives here rather than in tabnas/yaml because that repository does
//! not depend on alchemy, and this command composes the three components.
//! The fixtures are the sibling checkout's, the one `rs/Cargo.toml` takes
//! tabnas-yaml from by path, so they are the reader's own: the parity rows
//! of `test/spec/*.tsv` that parse, and every case of the vendored YAML
//! Test Suite that is not an error case.
//!
//! The inputs a reader defect misreads are a checked ledger,
//! [`READER_DEFECT`]: each must still come back different, so a fix to the
//! reader fails this test until its lines are deleted, and every input not
//! in it must come back the same. The render's output for each is valid
//! YAML 1.2; it is YAML's reader that misreads it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tabnas_alchemy::translate::Options;
use tabnas_dmog::translate::{self, Request};
use tabnas_transduce::{Code, Fail, Limits, Metrics};

/// The inputs YAML's reading misreads, each with the issue that records
/// the defect, named as this test names an input: `spec/<file>.tsv:<line>`,
/// or the YAML Test Suite case's id.
///
/// Empty since the reader fixed the two defects the pilot found
/// (tabnas/yaml#86, a quoted key at the start of a line after a block
/// sequence read into the sequence; tabnas/yaml#88, a flow sequence first
/// in an indented block sequence replacing it), and since tabnas/yaml
/// 0.5.28 holds transduce's pruning while a value with an anchor is open
/// (tabnas/yaml#138: an alias to an anchored sequence used as a key had
/// read through YAML's part as the text of the sequence emptied behind the
/// stream, `""` where the whole read gives `"1,2"`): every input comes back
/// the same. The ledger stays, so that a defect a later checkout brings is
/// recorded here rather than hidden.
const READER_DEFECT: &[(&str, u32)] = &[];

/// What a run wrote, shared with the test.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The root of the sibling tabnas/yaml checkout, the one `rs/Cargo.toml`
/// takes tabnas-yaml from, so the fixtures are the reader's own. Without
/// it, or without its fixtures, this fails rather than reading none.
fn yaml_checkout() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../yaml");
    assert!(
        root.join("test/spec").is_dir() && root.join("test/yaml-test-suite").is_dir(),
        "no tabnas/yaml checkout with its fixtures beside this one: {}",
        root.display()
    );
    root
}

/// A fixture cell's escapes decoded, as the fixture runners decode them:
/// `\n`, `\r`, `\t` and `\\`, and any other backslash kept.
fn unescape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every input the checkout's fixtures hold that is meant to parse, named.
fn inputs(root: &Path) -> Vec<(String, String)> {
    let mut inputs = Vec::new();
    let mut tsvs: Vec<PathBuf> = std::fs::read_dir(root.join("test/spec"))
        .expect("the checkout has test/spec")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "tsv"))
        .collect();
    tsvs.sort();
    for tsv in tsvs {
        let file = tsv.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&tsv).unwrap();
        let mut header = true;
        for (i, line) in text.lines().enumerate() {
            if line.is_empty() || (line.starts_with('#') && !line.contains('\t')) {
                continue;
            }
            if header {
                header = false;
                continue;
            }
            let mut cells = line.split('\t');
            let input = cells.next().unwrap_or("");
            let expected = cells.next().unwrap_or("");
            if expected.starts_with("ERROR") {
                continue;
            }
            inputs.push((format!("spec/{file}:{}", i + 1), unescape(input)));
        }
    }
    // The suite's cases as tabnas-yaml's own runner gathers them: each
    // directory holding an `in.yaml`, or its numbered sub-tests (`AB12/00`),
    // once each; the `name` and `tags` indexes link to the same cases.
    let suite = root.join("test/yaml-test-suite");
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(&suite).expect("the checkout vendors the suite") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        if dir.join("in.yaml").is_file() {
            cases.push(dir);
            continue;
        }
        for sub in std::fs::read_dir(&dir).unwrap() {
            let sub = sub.unwrap().path();
            let numbered = sub
                .file_name()
                .is_some_and(|n| n.to_string_lossy().chars().all(|c| c.is_ascii_digit()));
            if numbered && sub.join("in.yaml").is_file() {
                cases.push(sub);
            }
        }
    }
    cases.sort();
    for case in cases {
        if case.join("error").exists() {
            continue;
        }
        let name = case
            .strip_prefix(&suite)
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "/");
        let text = std::fs::read_to_string(case.join("in.yaml")).unwrap();
        inputs.push((name, text));
    }
    inputs
}

/// A value as text to compare: JSON, with the non-finite numbers the reader
/// builds from `.inf` and `.nan` spelled as they are, which JSON cannot. A
/// stream with no document has no value, which the render writes as null
/// (tabnas/yaml's DIVERGENCE.md: the Rust port keeps the undefined result
/// for an empty source alone).
fn read(text: &str) -> Option<String> {
    tabnas_yaml::parse(text).ok().map(|v| match v {
        tabnas::Value::Undefined => "null".to_string(),
        v => v.to_string(),
    })
}

/// `text` written as `dmog translate --from yaml --to yaml` writes it.
fn render(
    request: &Request<'_>,
    compiled: &translate::Compiled,
    text: &str,
) -> Result<String, Fail> {
    let out = Buffer::default();
    translate::run_compiled(
        request,
        compiled,
        text,
        Box::new(out.clone()),
        Metrics::new(),
    )?;
    let bytes = out.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("the render writes UTF-8"))
}

/// `text` written through `translate` as YAML: streamed through YAML's
/// part, and read whole (`--whole`) where the part refuses it part way, a
/// stream of several documents, a repeated key, two plain `<<` in one
/// mapping or a merge source that repeats a merged key, since the output
/// streams and nothing reads a document again (admin ADR-33).
fn written(
    (request, compiled): (&Request<'_>, &translate::Compiled),
    (whole, compiled_whole): (&Request<'_>, &translate::Compiled),
    text: &str,
) -> Result<String, String> {
    match render(request, compiled, text) {
        Err(f) if matches!(f.code, Code::StreamabilityUnknown | Code::DuplicateMember) => {
            render(whole, compiled_whole, text)
        }
        other => other,
    }
    .map_err(|e| format!("{e:?}"))
}

#[test]
fn every_yaml_fixture_written_as_yaml_reads_back_as_the_same_value() {
    let root = yaml_checkout();
    let yaml = translate::format("yaml").expect("tabnas-yaml names its render");
    let request = Request {
        from: yaml,
        to: yaml,
        path: None,
        options: Options::default(),
        program: None,
        whole: false,
        limits: Limits::default(),
    };
    let whole = Request {
        from: yaml,
        to: yaml,
        path: None,
        options: Options::default(),
        program: None,
        whole: true,
        limits: Limits::default(),
    };
    let inputs = inputs(&root);
    let total = inputs.len();
    assert!(total > 600, "only {total} inputs under {}", root.display());
    // The inputs are shared out over a few threads, each with the
    // translation compiled once, so the run takes seconds in a debug build.
    // A line every 100 inputs, and at least every 25 seconds, says how far
    // the sweep is, as every transient task reports.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 8));
    let chunks: Vec<&[(String, String)]> = inputs.chunks(total.div_ceil(threads)).collect();
    let started = Instant::now();
    let done = AtomicUsize::new(0);
    let reported = Mutex::new(Instant::now());
    let progress = |n: usize| {
        eprintln!(
            "round trip: {n} of {total} inputs ({}%), {}s",
            n * 100 / total,
            started.elapsed().as_secs()
        );
    };
    let results: Vec<(usize, usize, Vec<String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .iter()
            .map(|chunk| {
                let (request, whole, done, reported, progress) =
                    (&request, &whole, &done, &reported, &progress);
                scope.spawn(move || {
                    let compiled = translate::compile(request).unwrap_or_else(|f| panic!("{f}"));
                    let compiled_whole =
                        translate::compile(whole).unwrap_or_else(|f| panic!("{f}"));
                    let (mut same, mut unread) = (0, 0);
                    let mut differ: Vec<String> = Vec::new();
                    for (name, text) in chunk.iter() {
                        // The reader refuses some inputs the suite holds
                        // valid; its own ledger records them. They have
                        // nothing to write.
                        match read(text) {
                            None => unread += 1,
                            Some(value) => {
                                let written = written(
                                    (request, &compiled),
                                    (whole, &compiled_whole),
                                    text,
                                )
                                .unwrap_or_else(|e| panic!("{name}: {e}"));
                                match read(&written) {
                                    Some(back) if back == value => same += 1,
                                    back => differ.push(format!(
                                        "{name}\n  in:   {text:?}\n  out:  {written:?}\n  read: {}\n  back: {}",
                                        value,
                                        back.as_deref().unwrap_or("(does not parse)")
                                    )),
                                }
                            }
                        }
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        let mut last = reported.lock().unwrap();
                        if n % 100 == 0 || last.elapsed() >= Duration::from_secs(25) {
                            progress(n);
                            *last = Instant::now();
                        }
                    }
                    (same, unread, differ)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    progress(done.load(Ordering::Relaxed));
    let same: usize = results.iter().map(|r| r.0).sum();
    let unread: usize = results.iter().map(|r| r.1).sum();
    let differ: Vec<String> = results.into_iter().flat_map(|r| r.2).collect();
    eprintln!(
        "round trip: {total} inputs, {same} the same, {unread} the reader refuses, {} not",
        differ.len()
    );
    let named = |d: &String| d.lines().next().unwrap().to_string();
    let listed = |name: &str| READER_DEFECT.iter().any(|(n, _)| *n == name);
    let unlisted: Vec<&String> = differ.iter().filter(|d| !listed(&named(d))).collect();
    assert!(
        unlisted.is_empty(),
        "{} inputs come back different and are not in READER_DEFECT:\n{}",
        unlisted.len(),
        unlisted
            .iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let fixed: Vec<String> = READER_DEFECT
        .iter()
        .filter(|(n, _)| !differ.iter().any(|d| named(d) == *n))
        .map(|(n, issue)| format!("{n} (tabnas/yaml#{issue})"))
        .collect();
    assert!(
        fixed.is_empty(),
        "these now come back the same, or are no longer fixtures: delete them from \
         READER_DEFECT: {fixed:?}"
    );
}
