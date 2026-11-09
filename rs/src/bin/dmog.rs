//! The `dmog` command (data mogrify).
//!
//! ```text
//! dmog canon FILE       print the program in canonical form
//! dmog format FILE      print the program in layout form
//! dmog check FILE       parse, desugar, resolve, check and build the plan; print nothing and exit 0
//! dmog explain FILE     print the plan report
//! dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
//!                       run the program over the JSON document INPUT
//! dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
//!                [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT
//!                       write INPUT, read as FORMAT, in another format
//! dmog formats          print the formats translate reads and writes, as JSON
//! ```
//!
//! `FILE`, `PROGRAM` and `INPUT` may be `-` for standard input (one of
//! them per run). Nothing but the answer goes to standard output. A
//! failure is the `Fail` as one JSON object on standard error (`code`,
//! `message`, and `path`, `limit`, `row`, `col` when they apply, and
//! `output`: `"partial"` when bytes had reached standard output). The
//! exit status follows the code: 2 for a program that does not read,
//! resolve or check (`DSL_PARSE_ERROR`, `DSL_TYPE_ERROR`, `STREAM_REUSED`,
//! `STREAMABILITY_UNKNOWN`), for a usage error and for an unreadable
//! file; 1 for an input or protocol failure (from `check` too, when
//! building the plan reaches a `fail`); 5 for `RESOURCE_LIMIT_EXCEEDED`;
//! 3 for `OUTPUT_FAILED`; 6 for `ABORTED`. A standard error that cannot
//! be written loses the report, not the status.
//!
//! Every command runs on a thread of [`tabnas_alchemy::STACK_BYTES`], so
//! the checker's and the evaluator's bounds are reached before the
//! stack's end in a debug build as in a release one. `--max-output-bytes`
//! sets the writer's limit; the others are transduce's defaults.
//!
//! `run` reads `INPUT` as JSON through JSON's own transducer part, the
//! one `translate` reads a JSON document through: the part makes the
//! document's events of the parser's rule passes, and the engine's arrays
//! are emptied behind them. A document of another format is `translate
//! --with`'s business: it runs the program over `INPUT` read as `--from`.
//! The output goes through a coalescing writer to standard output and is
//! flushed once, at the end; a failure found after bytes were written
//! says so.
//!
//! `run` reads `INPUT`, a file or standard input, through a reader: the
//! JSON grammar declares `lex.window` (tabnas/parser's admin ADR-30), so
//! the parser holds a window of a long document rather than all of it.
//! `run --grammar` reads it the same way, a window at a time where the
//! grammar declares it and whole first otherwise, and empties the
//! engine's arrays behind the stream where transduce verifies the grammar
//! alike pruned and not, as `run` does for JSON. A read that fails, or
//! bytes that are not UTF-8, are met where they are, after any output the
//! document before them made, and fail as a file that cannot be read
//! does.
//!
//! `translate` reads `INPUT` with the grammar of `--from` and writes it in
//! `--to`, through the translation parts each format's package exports,
//! as `tabnas_dmog::translate` composes them: any of the formats
//! `dmog formats` lists into any other. `--path` takes a JSON array of
//! keys and indexes (`["people",0]`) and translates that value instead of
//! the document; `--key` names the member a root is wrapped under for a
//! format whose document must be an object (`items` by default); `--with`
//! runs a program over the input first, as `run` does, and writes its
//! export's events or table in `--to`. `INPUT`, a file or standard input,
//! is read through a reader on the route through the source's transducer
//! part, and whole on every other route ([`translate::Document`]), and the
//! output is written as the run goes: a document the part refuses part way
//! fails with its code after what it wrote, and `--whole` reads it whole,
//! as its grammar's value (admin ADR-33).
//!
//! The command is where alchemy, transduce and render meet: every program
//! is compiled with transduce's routers and render's renderers, which make
//! the stages its plan is lowered onto.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use tabnas_alchemy::translate::Options;
use tabnas_alchemy::{canonical, format, parse_file, Input, Program, Renderer};
use tabnas_dmog::translate::{self, Document, Format};
use tabnas_transduce::{Code, Fail, Limits, Metrics, ParserSink, Sink};

/// This crate's version, read from `rs/Cargo.toml`. It MUST equal
/// `ts/package.json` "version": admin/publish.sh rewrites both, and
/// `rs/tests/version_test.rs` fails the build if they drift.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "usage: dmog canon|format|check|explain FILE\n       dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT\n       dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY] [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT\n       dmog formats\n       (a FILE may be - for standard input)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The whole command runs on a thread of the stack the checker and the
    // evaluator are promised, so a program within their bounds fails with
    // a code rather than overflowing the main thread's smaller stack.
    let worker = std::thread::Builder::new()
        .name("dmog".into())
        .stack_size(tabnas_alchemy::STACK_BYTES)
        .spawn(move || command(&args));
    match worker {
        Ok(thread) => thread
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(error) => {
            report(&Fail::new(
                Code::ResourceLimitExceeded,
                format!("no thread could be started for the command: {error}"),
            ));
            ExitCode::from(5)
        }
    }
}

/// Run one command line and answer its exit status.
fn command(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(text) => {
            let mut out = io::stdout().lock();
            if out
                .write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .is_err()
            {
                return ExitCode::from(3);
            }
            ExitCode::SUCCESS
        }
        Err(exit) => {
            report(&exit.fail);
            ExitCode::from(exit.status)
        }
    }
}

/// The failure as one JSON object on standard error. A standard error
/// that cannot be written (a closed pipe) loses the report, not the
/// status: `eprintln!` would panic there and exit 101 in place of the
/// failure's own status.
fn report(fail: &Fail) {
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "{}", fail.to_json()).and_then(|()| err.flush());
}

/// A failure and the status it exits with: the code's, except that a
/// usage error and an unreadable file exit 2 although their code is
/// `INPUT_INVALID`, since they are the command line's fault, not the
/// document's.
struct Exit {
    fail: Fail,
    status: u8,
}

impl From<Fail> for Exit {
    fn from(fail: Fail) -> Exit {
        let status = match fail.code {
            Code::DslParseError
            | Code::DslTypeError
            | Code::StreamReused
            | Code::StreamabilityUnknown => 2,
            Code::ResourceLimitExceeded => 5,
            Code::OutputFailed => 3,
            Code::Aborted => 6,
            _ => 1,
        };
        Exit { fail, status }
    }
}

/// Compile `src`, named `file`, with transduce's routers and render's
/// renderers.
fn compile(src: &str, file: &str) -> Result<Program, Fail> {
    tabnas_alchemy::compile(
        src,
        file,
        Arc::new(tabnas_transduce::routers()),
        Arc::new(tabnas_render::renderers()),
    )
}

/// [`compile`] over the input the host declares: the parser's rule passes,
/// for `run --grammar`.
fn compile_with(src: &str, file: &str, input: Input) -> Result<Program, Fail> {
    tabnas_alchemy::compile_with(
        src,
        file,
        input,
        Arc::new(tabnas_transduce::routers()),
        Arc::new(tabnas_render::renderers()),
    )
}

fn usage() -> Exit {
    Exit {
        fail: Fail::input(USAGE),
        status: 2,
    }
}

fn read(file: &str) -> Result<String, Exit> {
    let mut text = String::new();
    let outcome = if file == "-" {
        io::stdin().lock().read_to_string(&mut text).map(|_| ())
    } else {
        std::fs::read_to_string(file).map(|read| text = read)
    };
    outcome.map_err(|error| Exit {
        fail: Fail::input(format!("cannot read {file}: {error}")),
        status: 2,
    })?;
    Ok(text)
}

/// `file`, or standard input for `-`, opened for a parse that reads it
/// through a reader. A file that cannot be opened is refused as [`read`]
/// refuses it; one that fails part way fails the run ([`unreadable`]).
fn open(file: &str) -> Result<Box<dyn Read + Send>, Exit> {
    if file == "-" {
        return Ok(Box::new(io::stdin()));
    }
    match std::fs::File::open(file) {
        Ok(opened) => Ok(Box::new(opened)),
        Err(error) => Err(Exit {
            fail: Fail::input(format!("cannot read {file}: {error}")),
            status: 2,
        }),
    }
}

/// A run's failure, where `file` could not be read as [`read`] reports
/// one: a read that failed, or bytes that are not UTF-8, which a parse
/// from a reader meets part way ([`translate::unreadable`]). It exits 2
/// and keeps the mark of any output written before it.
fn unreadable(file: &str, fail: Fail) -> Exit {
    match translate::unreadable(&fail) {
        Some(reason) => {
            let cannot = Fail::input(format!("cannot read {file}: {reason}"));
            Exit {
                fail: if fail.committed_output {
                    cannot.committed()
                } else {
                    cannot
                },
                status: 2,
            }
        }
        None => Exit::from(fail),
    }
}

/// The text to print for the command line, or the failure. `run` writes
/// its answer itself and answers nothing here.
fn run(args: &[String]) -> Result<String, Exit> {
    // The command is judged before any file is read, so an unknown one is
    // the usage error whatever the files hold, and never waits on standard
    // input to say so.
    let (command, rest) = args.split_first().ok_or_else(usage)?;
    match command.as_str() {
        "canon" | "format" | "check" | "explain" => {
            let [file] = rest else {
                return Err(usage());
            };
            let src = read(file)?;
            match command.as_str() {
                "canon" => {
                    let text = canonical(&parse_file(&src, file)?);
                    Ok(if text.is_empty() { text } else { text + "\n" })
                }
                "format" => Ok(format(&parse_file(&src, file)?)),
                "check" => Ok(compile(&src, file).map(|_| String::new())?),
                _ => Ok(compile(&src, file).map(|program| program.explain())?),
            }
        }
        "run" => {
            let options = RunOptions::parse(rest)?;
            let src = read(&options.program)?;
            // With --grammar the program reads the rule passes of INPUT
            // parsed as that format, and is compiled over them, as a
            // transducer part is; without it, the JSON document's events.
            let program = match options.grammar {
                Some(_) => compile_with(&src, &options.program, Input::ParserEvents)?,
                None => compile(&src, &options.program)?,
            };
            let limits = Limits {
                max_output_bytes: options.max_output_bytes,
                ..Limits::default()
            };
            // A renderer the program cannot take is refused before the
            // input is read, so the refusal never waits on standard input
            // or drains a large file to say so.
            let metrics = Metrics::new();
            match options.grammar {
                None => {
                    let sink = program.sink(
                        Box::new(io::stdout()),
                        options.render,
                        &limits,
                        metrics.clone(),
                    )?;
                    let input = open(&options.input)?;
                    execute(sink, metrics, input, limits)
                        .map_err(|fail| unreadable(&options.input, fail))?;
                }
                Some(grammar) => {
                    let sink = program.parser_sink(
                        Box::new(io::stdout()),
                        options.render,
                        &limits,
                        metrics.clone(),
                    )?;
                    let input = open(&options.input)?;
                    execute_passes(grammar, sink, metrics, input, limits)
                        .map_err(|fail| unreadable(&options.input, fail))?;
                }
            }
            Ok(String::new())
        }
        "translate" => {
            let options = TranslateOptions::parse(rest)?;
            let limits = Limits {
                max_output_bytes: options.max_output_bytes,
                ..Limits::default()
            };
            let path = match &options.path {
                Some(text) => Some(
                    translate::parse_path(text, &limits)
                        .map_err(|fail| Exit { fail, status: 2 })?,
                ),
                None => None,
            };
            let program = match &options.program {
                Some(file) => Some((file.as_str(), read(file)?)),
                None => None,
            };
            let request = translate::Request {
                from: options.from,
                to: options.to,
                path,
                options: Options {
                    key: options.key.unwrap_or_else(|| Options::default().key),
                },
                program: program.as_ref().map(|(file, text)| (*file, text.as_str())),
                whole: options.whole,
                limits,
            };
            // The composition is compiled before the input is read, so a
            // route that cannot be composed never waits on standard input,
            // and once.
            let compiled = translate::compile(&request)?;
            // Standard input is read once, through a reader on the part's
            // route, as a file is: nothing reads a document again.
            let document = if options.input == "-" {
                Document::Reader(Box::new(io::stdin()))
            } else {
                Document::File(Path::new(&options.input))
            };
            translate::run_document(
                &request,
                &compiled,
                document,
                Box::new(io::stdout()),
                Metrics::new(),
            )
            .map_err(|fail| unreadable(&options.input, fail))?;
            Ok(String::new())
        }
        "formats" => {
            if !rest.is_empty() {
                return Err(usage());
            }
            Ok(translate::formats_json() + "\n")
        }
        _ => Err(usage()),
    }
}

struct TranslateOptions {
    from: &'static translate::Format,
    to: &'static translate::Format,
    path: Option<String>,
    key: Option<String>,
    program: Option<String>,
    whole: bool,
    max_output_bytes: Option<u64>,
    input: String,
}

impl TranslateOptions {
    fn parse(args: &[String]) -> Result<TranslateOptions, Exit> {
        let named = |flag: &str, id: &str| {
            translate::format(id).ok_or_else(|| Exit {
                fail: Fail::input(format!("{flag} takes {}, not {id:?}", translate::names())),
                status: 2,
            })
        };
        let (mut from, mut to, mut path, mut key, mut program, mut max_output_bytes) =
            (None, None, None, None, None, None);
        let mut whole = false;
        let mut files = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let value = || args.get(i + 1).cloned().ok_or_else(usage);
            match args[i].as_str() {
                "--whole" => {
                    whole = true;
                    i += 1;
                    continue;
                }
                "--from" => from = Some(named("--from", &value()?)?),
                "--to" => to = Some(named("--to", &value()?)?),
                "--path" => path = Some(value()?),
                "--key" => key = Some(value()?),
                "--with" => program = Some(value()?),
                "--max-output-bytes" => {
                    let n = value()?;
                    max_output_bytes = Some(n.parse::<u64>().map_err(|_| Exit {
                        fail: Fail::input(format!(
                            "--max-output-bytes takes a number of bytes, not {n:?}"
                        )),
                        status: 2,
                    })?);
                }
                other if other.starts_with("--") => return Err(usage()),
                other => {
                    files.push(other.to_string());
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        let (Some(from), Some(to), [input]) = (from, to, files.as_slice()) else {
            return Err(usage());
        };
        if program.as_deref() == Some("-") && input == "-" {
            return Err(Exit {
                fail: Fail::input("only one of PROGRAM and INPUT may be - (standard input)"),
                status: 2,
            });
        }
        Ok(TranslateOptions {
            from,
            to,
            path,
            key,
            program,
            whole,
            max_output_bytes,
            input: input.clone(),
        })
    }
}

struct RunOptions {
    /// The format whose parser reads INPUT, its rule passes the program's
    /// input (`--grammar`); without one the program reads INPUT as a JSON
    /// document's events.
    grammar: Option<&'static Format>,
    render: Option<Renderer>,
    max_output_bytes: Option<u64>,
    program: String,
    input: String,
}

impl RunOptions {
    fn parse(args: &[String]) -> Result<RunOptions, Exit> {
        let mut grammar = None;
        let mut render = None;
        let mut max_output_bytes = None;
        let mut files = Vec::new();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--grammar" => {
                    let name = args.get(i + 1).ok_or_else(usage)?;
                    grammar = Some(translate::format(name).filter(|f| f.parses()).ok_or_else(
                        || Exit {
                            fail: Fail::input(format!(
                                "--grammar takes a format whose documents are parsed \
                                     ({}), not {name:?}",
                                translate::parser_names()
                            )),
                            status: 2,
                        },
                    )?);
                    i += 2;
                }
                "--render" => {
                    let name = args.get(i + 1).ok_or_else(usage)?;
                    render = Some(Renderer::named(name).ok_or_else(|| Exit {
                        fail: Fail::input(format!("--render takes csv or json, not {name:?}")),
                        status: 2,
                    })?);
                    i += 2;
                }
                "--max-output-bytes" => {
                    let n = args.get(i + 1).ok_or_else(usage)?;
                    max_output_bytes = Some(n.parse::<u64>().map_err(|_| Exit {
                        fail: Fail::input(format!(
                            "--max-output-bytes takes a number of bytes, not {n:?}"
                        )),
                        status: 2,
                    })?);
                    i += 2;
                }
                other if other.starts_with("--") => return Err(usage()),
                other => {
                    files.push(other.to_string());
                    i += 1;
                }
            }
        }
        let [program, input] = files.as_slice() else {
            return Err(usage());
        };
        if program == "-" && input == "-" {
            return Err(Exit {
                fail: Fail::input("only one of PROGRAM and INPUT may be - (standard input)"),
                status: 2,
            });
        }
        Ok(RunOptions {
            grammar,
            render,
            max_output_bytes,
            program: program.clone(),
            input: input.clone(),
        })
    }
}

/// Run the program over one JSON document, which `input` gives, into the
/// sink that writes its output. The document's events come from JSON's
/// own transducer part over the parser's rule passes, the engine's arrays
/// emptied behind them.
fn execute(
    sink: Box<dyn Sink + Send>,
    metrics: Arc<Metrics>,
    input: Box<dyn Read + Send>,
    limits: Limits,
) -> Result<(), Fail> {
    let json = translate::format("json")
        .ok_or_else(|| Fail::input("no JSON grammar to read the input"))?;
    let part = json
        .transducer_program()?
        .ok_or_else(|| Fail::input("the JSON grammar ships no transducer part"))?;
    let head = part.parser_sink_into(sink, &limits, metrics.clone())?;
    json.passes_from(input)
        .and_then(|source| {
            json.pruned(source)
                .limits(limits)
                .metrics(metrics.clone())
                .run_boxed(head)
                .0
        })
        .map(|_| ())
        .map_err(|fail| committed(fail, &metrics))
}

/// Run the program over the rule passes of one document, which `input`
/// gives, parsed with `grammar`, into the sink that writes its output.
fn execute_passes(
    grammar: &Format,
    sink: Box<dyn ParserSink + Send>,
    metrics: Arc<Metrics>,
    input: Box<dyn Read + Send>,
    limits: Limits,
) -> Result<(), Fail> {
    grammar
        .passes_from(input)
        .and_then(|source| {
            grammar
                .pruned(source)
                .limits(limits)
                .metrics(metrics.clone())
                .run_boxed(sink)
                .0
        })
        .map(|_| ())
        .map_err(|fail| committed(fail, &metrics))
}

/// A run's failure, marked as after output where bytes had reached
/// standard output: a failure the source found (bad JSON after the rows)
/// knows nothing of the output, and the writer's count says.
fn committed(fail: Fail, metrics: &Metrics) -> Fail {
    if Metrics::get(&metrics.output_bytes) > 0 && !fail.committed_output {
        fail.committed()
    } else {
        fail
    }
}
