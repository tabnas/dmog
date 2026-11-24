// tabnas-alchemy's shared fixture run.tsv, in its vendored test/spec, run through tabnas_support's runner as alchemy runs its other
// three: the loader, the escape codec, the `ERROR:<code>` contract and the
// row loop are the fleet's. A row's input is a program, run over the row's
// JSON document, and the expected column is the bytes the run writes, as
// a JSON string, or its failure. Running needs the routers and renderers a
// host passes in, transduce's and render's, which alchemy does not depend
// on; this crate composes all three, so the runner moved here.
//
// So did the full catalogue drift test: the per-row checks run in alchemy
// over reader.tsv, pipe.tsv and check.tsv, but that every line of the
// grammar document's own codes is met by some row needs run.tsv's rows as
// well, so the copy here runs every error row of all four files.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use tabnas_alchemy::shared::{Code, Fail, Limits, Metrics};
use tabnas_alchemy::{desugar, parse, Input, Output, Program, Renderer};
use tabnas_dmog::translate::{self, Format};
use tabnas_support::{is_error_expect, load_spec_dir, Failure, Row, Runner, SpecOptions};
use tabnas_transduce::{
    walk_datum, Datum, DatumBuilder, Duplicates, Flow, JsonEvent, OwnedJsonEvent, Sink,
};

use common::{compile, compile_with, json_into, spec_dir, text_value};

/// A program run over a JSON document: the bytes it writes, or the
/// failure. See alchemy's `test/AGENTS.md` for the columns.
///
/// Every row runs the library's text, the one implementation of its
/// compositions, and is held to its expected cell.
#[test]
fn run() {
    Runner::new_with_row(|program, row| {
        let doc = match row.unesc_named("doc") {
            doc if doc.is_empty() => "null".to_string(),
            doc => doc,
        };
        let render = row_render(row);
        run_program(program, &doc, render)
            .map(text_value)
            .map_err(|fail| run_failure(&fail))
    })
    .file(spec_dir().join("run.tsv"));
}

/// A row's `render` column: how a table or JSON-events result is rendered,
/// or the host's default.
fn row_render(row: &Row) -> Option<Renderer> {
    match row.named("render") {
        "" => None,
        name => Some(
            Renderer::named(name)
                .unwrap_or_else(|| panic!("{}: render is csv, json or empty", row.location())),
        ),
    }
}

/// Compile `program` and run it over `doc`, as `dmog run` does: on a
/// thread of `STACK_BYTES`, the document read through JSON's transducer
/// part, with the default limits.
fn run_program(program: &str, doc: &str, render: Option<Renderer>) -> Result<String, Fail> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(tabnas_alchemy::STACK_BYTES)
            .spawn_scoped(scope, || {
                let compiled = compile(program, "run")?;
                drive(&compiled, doc, render)
            })
            .expect("the run thread starts")
            .join()
            .expect("the run thread finishes")
    })
}

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn drive(program: &Program, doc: &str, render: Option<Renderer>) -> Result<String, Fail> {
    let limits = Limits::default();
    let metrics = Metrics::new();
    let buffer = Shared::default();
    let sink = program.sink(Box::new(buffer.clone()), render, &limits, metrics.clone())?;
    json_into(doc, &limits, metrics, sink)?;
    let bytes = buffer.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("the output is UTF-8"))
}

/// The code a run row pins: the finer code (the first word of the
/// message) for this crate's own codes, which carry one, and the
/// transduce or render code itself for every other failure, whose
/// message is free text (`fail "a: b"` must not pin `a`).
fn run_code(fail: &Fail) -> String {
    match fail.code {
        Code::DslParseError
        | Code::DslTypeError
        | Code::StreamReused
        | Code::StreamabilityUnknown => common::fail_code(fail),
        other => other.as_str().to_string(),
    }
}

fn run_failure(fail: &Fail) -> Failure {
    let mut failure = Failure::new(run_code(fail)).with_message(fail.to_string());
    if let (Some(row), Some(col)) = (fail.row, fail.column) {
        failure = failure.at(row as usize, col as usize);
    }
    failure
}

/// The failures a row's program meets, as the runners meet them: the
/// reader's (`reader.tsv`), the desugarer's (`pipe.tsv`) and compile's
/// (`check.tsv`, over the input its `protocol` column declares: empty for
/// JSON events, `parser` for the parser's rule passes), whose runners are
/// alchemy's, or a run's on both paths (`run.tsv`), whose runner is
/// above.
fn row_failures(file: &str, row: &Row) -> Vec<Fail> {
    let input = row.unesc(0);
    match file {
        "reader.tsv" => parse(&input).err().into_iter().collect(),
        "pipe.tsv" => parse(&input)
            .and_then(|program| desugar::program(program, &input))
            .err()
            .into_iter()
            .collect(),
        "check.tsv" => {
            let declared = match row.named("protocol") {
                "" => Input::JsonEvents,
                "parser" => Input::ParserEvents,
                other => panic!(
                    "{}: the protocol is empty or parser, not {other:?}",
                    row.location()
                ),
            };
            compile_with(&input, "check", declared)
                .err()
                .into_iter()
                .collect()
        }
        "transduce.tsv" => {
            let grammar = transduce_grammar(row);
            let doc = row.unesc_named("doc");
            let render = row_render(row);
            let duplicates = row_duplicates(row);
            transduce_program(&input, grammar, &doc, render, duplicates)
                .err()
                .into_iter()
                .collect()
        }
        _ => {
            let doc = match row.unesc_named("doc") {
                doc if doc.is_empty() => "null".to_string(),
                doc => doc,
            };
            let render = row_render(row);
            run_program(&input, &doc, render)
                .err()
                .into_iter()
                .collect()
        }
    }
}

/// The fixed parts of a template, between and around its `{name}`s.
fn fixed_parts(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut parts = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let name = bytes[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_lowercase() || **b == b'_')
                .count();
            if name > 0 && bytes.get(i + 1 + name) == Some(&b'}') {
                parts.push(&line[start..i]);
                i += name + 2;
                start = i;
                continue;
            }
        }
        i += 1;
    }
    parts.push(&line[start..]);
    parts
}

/// Whether `text` fills the template `line`: its fixed parts in order,
/// each `{name}` any text, none included.
fn fills(line: &str, text: &str) -> bool {
    let parts = fixed_parts(line);
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return text == line;
    }
    if text.len() < first.len() + last.len() || !text.starts_with(first) || !text.ends_with(last) {
        return false;
    }
    let end = text.len() - last.len();
    let mut at = first.len();
    for part in &parts[1..parts.len() - 1] {
        match text[at..end].find(part) {
            Some(found) => at += found + part.len(),
            None => return false,
        }
    }
    true
}

/// Whether `text` is an instance of the template `line`. The engine trims
/// the messages it writes from a template, so a `{name}` that ends a line
/// may take the space before it with it.
fn instance_of(line: &str, text: &str) -> bool {
    if fills(line, text) {
        return true;
    }
    match (fixed_parts(line).last(), line.rfind('{')) {
        (Some(&""), Some(open)) if line.ends_with('}') => fills(line[..open].trim_end(), text),
        _ => false,
    }
}

/// Every failure alchemy's shared fixtures meet, in all four files, is
/// declared in the alchemy grammar document: the finer code that leads the
/// message is a key of the installed `options.error`, the engine's and the
/// alchemy grammar's, and the text after it is a line of that entry, each
/// `{name}` standing for what the raising site fills in (a failure raised
/// inside the standard library ends with its position there,
/// ` (at stdlib/...)`, which is not part of the text). Each line of each
/// code the alchemy grammar adds to the engine's is the most specific line
/// some row meets, so the document holds no text nothing raises, and each
/// of those codes has a hint. A finer code travels with one code wherever
/// it is raised (`bad_let` is a `DSL_PARSE_ERROR` from the desugarer and
/// from the resolver alike). A raising site whose code or text drifts from
/// the document fails here. alchemy's own copy holds the per-row checks
/// over the three files it runs; the coverage needs run.tsv's rows too.
#[test]
fn the_raised_messages_match_the_document() {
    let installed = tabnas_alchemy::make().config();
    let engine = tabnas::Tabnas::new().config().error;
    let mut met = BTreeSet::new();
    let mut codes: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut failures = 0;
    for spec in load_spec_dir(spec_dir(), &SpecOptions::default()).expect("the fixtures load") {
        for row in &spec.rows {
            if !is_error_expect(row.col(1)) {
                continue;
            }
            for fail in row_failures(&spec.file, row) {
                if !matches!(
                    fail.code,
                    Code::DslParseError
                        | Code::DslTypeError
                        | Code::StreamReused
                        | Code::StreamabilityUnknown
                ) {
                    continue;
                }
                failures += 1;
                let Some((code, text)) = fail.message.split_once(": ") else {
                    problems.push(format!(
                        "{}: {} has no finer code: {}",
                        row.location(),
                        fail.code.as_str(),
                        fail.message
                    ));
                    continue;
                };
                codes
                    .entry(code.to_string())
                    .or_default()
                    .insert(fail.code.as_str());
                let text = match text.rfind(" (at stdlib/") {
                    Some(library) if text.ends_with(')') => &text[..library],
                    _ => text,
                };
                let Some(entry) = installed.error.get(code) else {
                    problems.push(format!(
                        "{}: {code} is not declared in options.error",
                        row.location()
                    ));
                    continue;
                };
                // The most specific line, the first of equals.
                let fixed = |line: &str| fixed_parts(line).concat().len();
                let mut best: Option<&str> = None;
                for line in entry.split('\n').filter(|line| instance_of(line, text)) {
                    if best.is_none_or(|known| fixed(line) > fixed(known)) {
                        best = Some(line);
                    }
                }
                match best {
                    Some(line) => {
                        met.insert(format!("{code}\n{line}"));
                    }
                    None => problems.push(format!(
                        "{}: no line of options.error.{code} is {text:?}",
                        row.location()
                    )),
                }
            }
        }
    }
    assert!(failures > 0, "the fixtures meet failures");
    for (code, uppers) in &codes {
        if uppers.len() > 1 {
            problems.push(format!(
                "{code} is raised as {}; a finer code has one code",
                uppers.iter().copied().collect::<Vec<_>>().join(" and ")
            ));
        }
    }
    let own: BTreeMap<&String, &String> = installed
        .error
        .iter()
        .filter(|(code, _)| !engine.contains_key(*code))
        .collect();
    assert!(!own.is_empty(), "the grammar declares codes of its own");
    for (code, entry) in own {
        if !installed.hint.contains_key(code) {
            problems.push(format!("options.hint.{code} is not declared"));
        }
        for line in entry.split('\n') {
            if !met.contains(&format!("{code}\n{line}")) {
                problems.push(format!(
                    "options.error.{code}: no fixture row meets {line:?}"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// A program over a grammar's rule passes, run over a document in that
/// grammar: the bytes it writes, or the failure. See alchemy's
/// `test/AGENTS.md` for the columns. A row's program is a
/// transducer, its events the document's, and the runner holds it to
/// that: where the program answers JSON events, the grammar accepts the
/// document and no member repeats, the events it writes are the walk of
/// the grammar's own value, a number's lexeme aside ([`differential`]).
#[test]
fn transduce() {
    Runner::new_with_row(|program, row| {
        let grammar = transduce_grammar(row);
        let doc = row.unesc_named("doc");
        let render = row_render(row);
        let duplicates = row_duplicates(row);
        let outcome = transduce_program(program, grammar, &doc, render, duplicates);
        if let Ok((Output::JsonEvents, written)) = &outcome {
            differential(grammar, &doc, written).map_err(Failure::message)?;
        }
        outcome
            .map(|(_, text)| text_value(text))
            .map_err(|fail| run_failure(&fail))
    })
    .file(spec_dir().join("transduce.tsv"));
}

/// The policy a transduce row's `duplicates` names for a member name an
/// object repeats within a captured value: empty is the default, `first`
/// keeps the earlier value, `last` the later.
fn row_duplicates(row: &Row) -> Duplicates {
    match row.named("duplicates") {
        "" => Duplicates::Reject,
        "first" => Duplicates::FirstWins,
        "last" => Duplicates::LastWins,
        other => panic!(
            "{}: duplicates is empty, first or last, not {other:?}",
            row.location()
        ),
    }
}

/// The format a transduce row's `grammar` names, whose parser reads its
/// document: one of the command's own, with a parser.
fn transduce_grammar(row: &Row) -> &'static Format {
    let name = row.named("grammar");
    translate::format(name)
        .filter(|format| format.parses())
        .unwrap_or_else(|| {
            panic!(
                "{}: grammar names a format whose documents are parsed ({}), not {name:?}",
                row.location(),
                translate::parser_names()
            )
        })
}

/// Compile `program` over the parser's rule passes and run it over `doc`,
/// parsed with `grammar`, as `dmog run --grammar` does: on a thread of
/// `STACK_BYTES`, with the default limits. The program's output kind rides
/// along, for the differential.
fn transduce_program(
    program: &str,
    grammar: &'static Format,
    doc: &str,
    render: Option<Renderer>,
    duplicates: Duplicates,
) -> Result<(Output, String), Fail> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(tabnas_alchemy::STACK_BYTES)
            .spawn_scoped(scope, || {
                let mut compiled = compile_with(program, "transduce", Input::ParserEvents)?;
                if duplicates != Duplicates::Reject {
                    compiled = compiled.with_duplicates(duplicates)?;
                }
                let output = compiled.output();
                drive_passes(&compiled, grammar, doc, render).map(|text| (output, text))
            })
            .expect("the run thread starts")
            .join()
            .expect("the run thread finishes")
    })
}

fn drive_passes(
    program: &Program,
    grammar: &Format,
    doc: &str,
    render: Option<Renderer>,
) -> Result<String, Fail> {
    let limits = Limits::default();
    let metrics = Metrics::new();
    let buffer = Shared::default();
    let sink = program.parser_sink(Box::new(buffer.clone()), render, &limits, metrics.clone())?;
    grammar
        .passes(doc)?
        .limits(limits)
        .metrics(metrics)
        .run_boxed(sink)
        .0?;
    let bytes = buffer.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("the output is UTF-8"))
}

/// The events of a document, collected into a value under a duplicate
/// policy, where the run that fed them leaves it.
#[derive(Clone)]
struct Collect(Arc<Mutex<DatumBuilder>>);

impl Sink for Collect {
    fn event(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        if !matches!(ev, JsonEvent::End) {
            self.0.lock().unwrap().event(ev)?;
        }
        Ok(Flow::Continue)
    }
}

/// The differential rule of `transduce.tsv`: the events a row's program
/// wrote, read back from its JSON, are the walk of the grammar's own value
/// of the document, a number's lexeme aside. A document the grammar
/// refuses has no value to walk, and a member the program streamed twice
/// is kept once by the value: neither row has an oracle, and both pass
/// here, held by their expected column alone.
fn differential(grammar: &Format, doc: &str, written: &str) -> Result<(), String> {
    let Ok(value) = grammar.read(doc, &Limits::default()) else {
        return Ok(());
    };
    let collect = Collect(Arc::new(Mutex::new(DatumBuilder::new(
        usize::MAX,
        "max_capture_bytes",
        Duplicates::Reject,
    ))));
    let outcome = json_into(
        written,
        &Limits::default(),
        Metrics::new(),
        Box::new(collect.clone()),
    );
    let read = match outcome {
        Err(fail) if fail.code == Code::DuplicateMember => return Ok(()),
        Err(fail) => {
            return Err(format!(
                "the output {written:?} does not read back as JSON: {fail}"
            ))
        }
        Ok(_) => collect
            .0
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| format!("the output {written:?} holds no value"))?,
    };
    let (written, walked) = (plain_events(&read), plain_events(&value));
    if written == walked {
        Ok(())
    } else {
        Err(format!(
            "the transducer's events are not the walk of the grammar's value\n  written: {written:?}\n  walked:  {walked:?}"
        ))
    }
}

/// A value's events, a number's lexeme dropped.
fn plain_events(value: &Datum) -> Vec<OwnedJsonEvent> {
    let mut events: Vec<OwnedJsonEvent> = Vec::new();
    walk_datum(value, &mut events).expect("a value walks");
    for event in &mut events {
        if let OwnedJsonEvent::Number { lexeme, .. } = event {
            *lexeme = None;
        }
    }
    events
}
