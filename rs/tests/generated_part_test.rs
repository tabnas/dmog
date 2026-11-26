// The transducer part tabnas-abnf generates for an ABNF grammar (step 8
// of alchemy's `docs/streaming-plan.md`), run as a host runs it: compiled
// by alchemy, over the grammar's rule passes from transduce's
// `ParserEventSource`. Over every grammar of abnf's
// `alignment-abnf-ast.tsv`, a generated part writes the value the compiled
// grammar builds, and the generator declares
// every other grammar read whole, for the reason it gives for a start rule
// with no annotation. The part's events reach a renderer the program
// builds (`Program::parser_sink`) and a sink of the host's own
// (`Program::parser_sink_into`), which is how aless runs a part in front
// of its own stages.

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tabnas::{Options, RewindOptions, Tabnas};
use tabnas_abnf::{abnf_convert, abnf_transduce, GrammarSpec, TransducePart};
use tabnas_alchemy::shared::{Fail, JsonOptions, Limits, Metrics, Sink};
use tabnas_alchemy::{Input, Program, Renderer};
use tabnas_support::{load_spec, SpecOptions};

/// A writer the test keeps a handle on after the sink took it.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Shared {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("utf-8 output")
    }
}

/// abnf's shared fixtures, vendored. Missing files fail, naming where
/// they were looked for.
fn abnf_spec_dir() -> PathBuf {
    let dir = common::vendored().join("abnf/test/spec");
    assert!(
        dir.is_dir(),
        "no vendored abnf test/spec at {}: run tools/vendor-fixtures.sh",
        dir.display()
    );
    dir
}

/// An engine carrying the grammar, as abnf's own tests install one.
fn engine(spec: &GrammarSpec) -> Tabnas {
    let options = Options {
        rewind: RewindOptions {
            history: Some(4096),
        },
        ..Options::default()
    };
    let mut parser = Tabnas::with_options(options);
    spec.install(&mut parser).expect("the grammar installs");
    parser
}

/// JSON text written again in one form, members in their order.
fn canonical(text: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(text).unwrap_or_else(|e| panic!("the output is JSON: {e}: {text}"));
    serde_json::to_string(&value).unwrap()
}

/// One grammar of the fixture whose generated part streams: the
/// grammar's spec, a document, the value the grammar builds of it as
/// JSON, and the part compiled.
struct Case {
    grammar: String,
    input: String,
    spec: GrammarSpec,
    want: String,
    program: Program,
}

/// Every grammar of abnf's `alignment-abnf-ast.tsv` with its generated
/// part, and how many the generator declares read whole: each of those
/// for the reason it gives for a start rule with no annotation, never an
/// annotated grammar.
fn cases() -> (Vec<Case>, usize) {
    let fixture = load_spec(
        abnf_spec_dir().join("alignment-abnf-ast.tsv"),
        &SpecOptions::default(),
    )
    .expect("abnf's fixture loads");
    let (mut cases, mut whole) = (Vec::new(), 0);
    for row in &fixture.rows {
        let grammar = row.unesc_named("grammar");
        let input = row.unesc_named("input");
        let annotated = grammar.contains("@object") || grammar.contains("@array");
        let (entry, source) = match abnf_transduce(&grammar, "g", None)
            .unwrap_or_else(|e| panic!("{grammar:?} converts: {e}"))
        {
            TransducePart::Part { entry, source } => (entry, source),
            TransducePart::Whole(reason) => {
                assert!(
                    !annotated && reason.starts_with("the start rule "),
                    "{grammar:?} is read whole: {reason}"
                );
                whole += 1;
                continue;
            }
        };
        let spec = abnf_convert(&grammar, None).expect("the grammar converts");
        let value = engine(&spec)
            .parse(&input)
            .unwrap_or_else(|e| panic!("{grammar:?} parses {input:?}: {e}"));
        let want = serde_json::to_string(&value.to_json()).unwrap();
        let src = format!("{source}\ndef export [input] ({entry} input)\n");
        let program = common::compile_with(&src, "generated.alc", Input::ParserEvents)
            .unwrap_or_else(|f| panic!("the part of {grammar:?} compiles: {f:?}\n{source}"));
        cases.push(Case {
            grammar,
            input,
            spec,
            want,
            program,
        });
    }
    assert!(cases.len() > 10, "{} grammars streamed", cases.len());
    (cases, whole)
}

/// One document's rule passes into `sink`, the program's head.
fn drive(
    spec: &GrammarSpec,
    input: &str,
    sink: Result<Box<dyn tabnas_alchemy::shared::ParserSink + Send>, Fail>,
) -> Result<(), Fail> {
    let (outcome, _) =
        tabnas_transduce::ParserEventSource::new(engine(spec), input).run_boxed(sink?);
    outcome.map(drop)
}

#[test]
fn a_generated_part_writes_the_value_its_grammar_builds() {
    let (cases, whole) = cases();
    let mut failures = Vec::new();
    for case in &cases {
        {
            let program = &case.program;
            let out = Shared::default();
            let sink = program.parser_sink(
                Box::new(out.clone()),
                Some(Renderer::Json),
                &Limits::default(),
                Metrics::new(),
            );
            match drive(&case.spec, &case.input, sink) {
                Ok(()) if canonical(&out.text()) == case.want => {}
                got => failures.push(format!(
                    "{:?} on {:?}\n  part:  {got:?} {}\n  value: {}",
                    case.grammar,
                    case.input,
                    out.text(),
                    case.want
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "generated parts: {} grammars write their values, {whole} read whole",
        cases.len()
    );
}

/// The parts again, their events into a sink of the host's own
/// (`Program::parser_sink_into`), as aless runs a format's part in front
/// of stages of its own: here render's JSON renderer, which the host
/// builds itself, writes the value each grammar builds. A sink of the
/// host's own takes the events as the source's
/// limits hold them, so a document nested past `max_depth` is refused as
/// the limit's.
#[test]
fn a_generated_parts_events_reach_a_hosts_own_sink() {
    let renderers = common::renderers();
    let limits = Limits::default();
    let (cases, _) = cases();
    let mut failures = Vec::new();
    for case in &cases {
        {
            let program = &case.program;
            let out = Shared::default();
            let text = renderers.write_out(Box::new(out.clone()), &limits, Metrics::new());
            let own: Box<dyn Sink + Send> = renderers.json(
                text,
                JsonOptions {
                    indent: None,
                    trailing_newline: true,
                },
            );
            let sink = program.parser_sink_into(own, &limits, Metrics::new());
            match drive(&case.spec, &case.input, sink) {
                Ok(()) if canonical(&out.text()) == case.want => {}
                got => failures.push(format!(
                    "{:?} on {:?}\n  part:  {got:?} {}\n  value: {}",
                    case.grammar,
                    case.input,
                    out.text(),
                    case.want
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // The source's limits on the part's events, as on a source's own: a
    // depth of one under a limit of none.
    let case = &cases[0];
    let tight = Limits {
        max_depth: 0,
        ..Limits::default()
    };
    {
        let program = &case.program;
        let own: Box<dyn Sink + Send> = renderers.json(
            renderers.write_out(Box::new(Shared::default()), &tight, Metrics::new()),
            JsonOptions {
                indent: None,
                trailing_newline: true,
            },
        );
        let fail = drive(
            &case.spec,
            &case.input,
            program.parser_sink_into(own, &tight, Metrics::new()),
        )
        .expect_err("a part's events are held to the source's limits");
        assert_eq!(
            fail.limit.as_ref().map(|l| l.name),
            Some("max_depth"),
            "{fail}"
        );
    }
}
