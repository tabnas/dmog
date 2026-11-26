// tabnas-alchemy's public API end to end: the spec's worked example
// (section 5) run through the spec's program (sections 12.1 and 13.4),
// byte for byte; the streaming behaviours of the
// spec's section 19.5 that apply to a run over one document; the `json`
// echo of every fixture against the walk's own rendering; and `records` of
// a table round-tripping through JSON. Moved here from alchemy's tests:
// every one runs on transduce's routers and render's renderers.

mod common;
#[path = "../../test/fixtures/transduce/rs/tests/support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};

use tabnas_alchemy::shared::{
    replay, AbortFlag, Code, Fail, JsonOptions, Limits, Metrics, OwnedJsonEvent,
};
use tabnas_alchemy::{Output, Program, Renderer};
use tabnas_render::{JsonRenderer, StringOut, WriteOut};
use tabnas_transduce::ParserSource;

use common::{compile, json_into};

/// The spec's worked example: aless's `tests/fixtures/records.json`,
/// byte for byte (329 bytes; the metadata before the rows; Bob's members
/// in another order; `50.25` and `72` as written).
const RECORDS: &str = r#"{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]},"payload":{"deep":{"records":[{"id":123,"person":{"name":"Alice"},"account":{"balance":50.25}},{"account":{"balance":72},"person":{"name":"Bob"},"id":456}]}}}}"#;

const EXPECTED_CSV: &str =
    "\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n";

/// The spec's program (sections 12.1 and 13.4).
const PROGRAM: &str = "def column-from-meta [source]
  record
    entry :label (get \"title\" source)
    entry :source
      as-path
        get \"path\" source

def api-binding
  record
    entry :columns
      path \"response\" \"metadata\" \"fields\"
    entry :rows
      path \"response\" \"payload\" \"deep\" \"records\" each-index
    entry :column column-from-meta

def api-table [input]
  table-from-json api-binding input

def export [input]
  pipe input
    api-table
    csv csv-options
";

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

/// What a host does: read `text` as `dmog run` reads its input, push
/// the events into the program's sink, and mark a failure as leaving
/// partial output when `output_bytes` says bytes reached the writer.
fn drive(
    program: &Program,
    text: &str,
    render: Option<Renderer>,
    limits: &Limits,
) -> (Result<(), Fail>, String) {
    let metrics = Metrics::new();
    let buffer = Shared::default();
    let sink = match program.sink(Box::new(buffer.clone()), render, limits, metrics.clone()) {
        Ok(sink) => sink,
        Err(f) => return (Err(f), String::new()),
    };
    let outcome = json_into(text, limits, metrics.clone(), sink);
    let outcome = outcome.map(|_| ()).map_err(|f| {
        if Metrics::get(&metrics.output_bytes) > 0 && !f.committed_output {
            f.committed()
        } else {
            f
        }
    });
    let bytes = buffer.0.lock().unwrap().clone();
    (outcome, String::from_utf8(bytes).expect("utf-8 output"))
}

fn ok(program: &Program, text: &str, render: Option<Renderer>) -> String {
    let (outcome, out) = drive(program, text, render, &Limits::default());
    outcome.unwrap_or_else(|f| panic!("{f}"));
    out
}

fn err(program: &Program, text: &str, limits: &Limits) -> (Fail, String) {
    let (outcome, out) = drive(program, text, None, limits);
    (outcome.expect_err("the run fails"), out)
}

fn spec_program() -> Program {
    compile(PROGRAM, "export.alc").expect("the spec's program compiles")
}

/// Acceptance 3: the worked example prints the spec's bytes.
#[test]
fn the_worked_example_prints_the_spec_csv() {
    {
        let program = spec_program();
        assert_eq!(program.output(), Output::Text);
        assert_eq!(
            program.row_selector().unwrap().to_string(),
            ".response.payload.deep.records[*]"
        );
        assert_eq!(ok(&program, RECORDS, None), EXPECTED_CSV);
    }
}

fn shaped(records: &str) -> String {
    let meta = support::METADATA;
    format!(
        r#"{{"response":{{"metadata":{meta},"payload":{{"deep":{{"records":[{records}]}}}}}}}}"#
    )
}

/// Spec 19.5: metadata after rows is rejected under the metadata-first
/// policy, before any row is retained, and no CSV is written.
#[test]
fn metadata_after_rows_is_an_input_order_violation() {
    let meta = support::METADATA;
    let doc = format!(
        r#"{{"response":{{"payload":{{"deep":{{"records":[{}]}}}},"metadata":{meta}}}}}"#,
        support::record(1)
    );
    {
        let program = spec_program();
        let (fail, out) = err(&program, &doc, &Limits::default());
        assert_eq!(fail.code, Code::InputOrderViolation);
        assert!(!fail.committed_output);
        assert_eq!(out, "");
    }
}

/// Spec 19.5: cells follow the schema's order whatever the row's.
#[test]
fn cells_follow_schema_order_not_member_order() {
    let doc = shaped(r#"{"account":{"balance":1},"person":{"name":"z"},"id":2}"#);
    {
        let program = spec_program();
        assert_eq!(
            ok(&program, &doc, None),
            "\"Identifier\",\"Full name\",\"Balance\"\r\n\"2\",\"z\",\"1\"\r\n"
        );
    }
}

/// Spec 19.5: no matching rows is a valid empty table: the header alone.
#[test]
fn no_matching_rows_prints_the_header_only() {
    let meta = support::METADATA;
    for doc in [
        shaped(""),
        format!(r#"{{"response":{{"metadata":{meta}}}}}"#),
    ] {
        {
            let program = spec_program();
            assert_eq!(
                ok(&program, &doc, None),
                "\"Identifier\",\"Full name\",\"Balance\"\r\n"
            );
        }
    }
}

/// Spec 19.5: invalid trailing input fails the run after rows were
/// exported; with enough rows to pass the writer's budget, the failure
/// says the output is partial, and what was committed is whole records.
#[test]
fn invalid_trailing_input_fails_after_rows_were_exported() {
    let whole = support::records_json(2000);
    let doc = format!("{whole} x");
    {
        let program = spec_program();
        let full = ok(&program, &whole, None);
        let (fail, out) = err(&program, &doc, &Limits::default());
        assert_eq!(fail.code, Code::InputInvalid);
        assert!(fail.committed_output);
        assert!(out.starts_with("\"Identifier\",\"Full name\",\"Balance\"\r\n"));
        assert!(out.len() > 32 * 1024);
        // The CSV renderer marks the end of each row, and the writer holds
        // a row until its end (tabnas/alchemy#29), where it once drained
        // wherever its budget fell (spec 17.4): what was committed is a
        // prefix of the whole output, and ends with a whole row.
        assert!(full.starts_with(&out));
        assert!(out.len() < full.len());
        assert!(out.ends_with("\r\n"));
    }
    // A small document: the rows were buffered, not committed, so nothing
    // reached the writer and the failure says so.
    let (fail, out) = err(&spec_program(), &format!("{RECORDS} x"), &Limits::default());
    assert_eq!(fail.code, Code::InputInvalid);
    assert!(!fail.committed_output);
    assert_eq!(out, "");
}

/// Spec 19.5: a very large selected row fails clearly under the limit
/// that bounds it: the library's `table-from-json` captures its rows
/// under `max_record_bytes` (`capture :row ... :max_record_bytes`), so the
/// generic `max_capture_bytes` does not decide.
#[test]
fn a_very_large_selected_row_names_the_limit() {
    let big = format!(
        r#"{{"id":1,"person":{{"name":"{}"}},"account":{{"balance":2}}}}"#,
        "x".repeat(4096)
    );
    let doc = shaped(&format!("{},{big}", support::record(0)));
    let limits = Limits {
        max_record_bytes: 1024,
        ..Limits::default()
    };
    {
        let program = spec_program();
        let (fail, _) = err(&program, &doc, &limits);
        assert_eq!(fail.code, Code::ResourceLimitExceeded);
        assert_eq!(fail.limit.as_ref().unwrap().name, "max_record_bytes");
        assert_eq!(
            fail.path.as_deref(),
            Some(".response.payload.deep.records[1]")
        );
    }
    // A small generic capture limit does not decide: the rows print.
    let capture = Limits {
        max_capture_bytes: 64,
        ..Limits::default()
    };
    {
        let program = spec_program();
        let (outcome, out) = drive(&program, RECORDS, None, &capture);
        outcome.unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(out, EXPECTED_CSV);
    }
}

/// `string-join` builds one string from a vector of strings, the
/// separator between them: a cell from the runs of a Markdown cell, a
/// `fail` message that names a key. An item that is not a string is a
/// type error where the plan is built, and the joined string is held to
/// `max_scalar_bytes` before it is built.
#[test]
fn string_join_builds_one_string_from_several() {
    let program = compile(
        "def export [input]\n  concat\n    string-join \", \" [\"a\" \"b\" \"c\"]\n    \"|\"\n    string-join \"-\" []\n    \"|\"\n    string-join \"\" [\"x\"]\n    \"|\"\n    string-join \" \" [(quoted \"k\") \"holds\" (scalar-text csv-options 1.5)]\n    \"\\n\"\n",
        "join.alc",
    )
    .unwrap();
    assert_eq!(program.output(), Output::Text);
    let (outcome, out) = drive(&program, "null", None, &Limits::default());
    outcome.unwrap();
    assert_eq!(out, "a, b, c||x|\"k\" holds 1.5\n");
    // A failure that names a key, built from the parts.
    let fail = compile(
        "def export [input] (let [m (fail (string-join \" \" [\"no value under\" (quoted \"k\")]))] (json input))",
        "named.alc",
    )
    .unwrap_err();
    assert_eq!(fail.code, Code::InputInvalid, "{fail}");
    assert_eq!(fail.message, "no value under \"k\"", "{fail}");
    // The joined string is one scalar, held to max_scalar_bytes: here one
    // built at the end of a scan over the events, under the run's limits.
    let program = compile(
        "def step [s e] (transition (push \"abcd\" s) [])\ndef fin [s] [(string-join \"\" s)]\ndef export [input]\n  join \"\" (scan-emit [] step fin (events input))\n",
        "big.alc",
    )
    .unwrap();
    let limits = Limits {
        max_scalar_bytes: 16,
        ..Limits::default()
    };
    let (fail, out) = err(&program, "[1,2,3,4,5,6,7,8]", &limits);
    assert_eq!(fail.code, Code::ResourceLimitExceeded, "{fail}");
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_scalar_bytes");
    assert!(fail.message.contains("string-join"), "{fail}");
    assert_eq!(out, "");
}

/// A missing cell under the standard options is `MISSING_VALUE`, and a
/// null is the empty string.
#[test]
fn missing_and_null_cells_follow_the_options() {
    {
        let program = spec_program();
        let (fail, _) = err(
            &program,
            &shaped(r#"{"id":1,"person":{},"account":{"balance":2}}"#),
            &Limits::default(),
        );
        assert_eq!(fail.code, Code::MissingValue);
        assert_eq!(
            ok(
                &program,
                &shaped(r#"{"id":null,"person":{"name":"n"},"account":{"balance":null}}"#),
                None
            ),
            "\"Identifier\",\"Full name\",\"Balance\"\r\n\"\",\"n\",\"\"\r\n"
        );
    }
}

fn fixtures() -> Vec<PathBuf> {
    let dir = common::vendored().join("transduce/rs/tests/fixtures");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "transduce's fixtures are vendored, at {}: {e}",
                dir.display()
            )
        })
        .map(|e| e.expect("an entry").path())
        .collect();
    paths.sort();
    paths
}

fn grammar_for(path: &Path) -> Option<fn() -> tabnas::Tabnas> {
    match path.extension().and_then(|e| e.to_str())? {
        "json" => Some(tabnas_json::make),
        "jsonl" => Some(tabnas_jsonl::make),
        "yaml" => Some(tabnas_yaml::make),
        "csv" | "tsv" => Some(tabnas_csv::make),
        _ => None,
    }
}

/// `json input` echoes every fixture as the JSON renderer renders the
/// walk's events: the plan passes the events through untouched.
#[test]
fn json_echo_of_every_fixture_equals_the_walks_rendering() {
    let echo = compile("def export [input] (json input)", "echo.alc").unwrap();
    assert_eq!(echo.output(), Output::Text);
    let identity = compile("def export [input] input", "id.alc").unwrap();
    assert_eq!(identity.output(), Output::JsonEvents);
    let mut compared = 0;
    for path in fixtures() {
        let Some(make) = grammar_for(&path) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap();
        let (outcome, events) =
            ParserSource::new(make(), &text).run_owned(Vec::<OwnedJsonEvent>::new());
        if outcome.is_err() {
            continue;
        }
        let mut reference = JsonRenderer::new(
            StringOut::new(),
            JsonOptions {
                indent: None,
                trailing_newline: true,
            },
        );
        if replay(&events, &mut reference).is_err() {
            // A document the renderer refuses (a non-finite number, say)
            // is refused the same way through the program; not compared.
            continue;
        }
        let expected = reference.into_inner().into_string();
        for program in [&echo, &identity] {
            let buffer = Shared::default();
            let mut sink = program
                .sink(
                    Box::new(buffer.clone()),
                    None,
                    &Limits::default(),
                    Metrics::new(),
                )
                .unwrap();
            replay(&events, &mut sink).unwrap_or_else(|f| panic!("{}: {f}", path.display()));
            let bytes = buffer.0.lock().unwrap().clone();
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                expected,
                "{}",
                path.display()
            );
        }
        compared += 1;
    }
    assert!(compared > 10, "{compared} fixtures compared");
}

/// `records` of a table is JSON events: rendered as JSON they read back
/// as the rows, keyed by label, with the lexemes kept.
#[test]
fn records_of_a_table_round_trips() {
    let program = compile(
        &PROGRAM.replace("    csv csv-options\n", "    records\n    json\n"),
        "records.alc",
    )
    .unwrap();
    assert_eq!(program.output(), Output::Text);
    let expected = r#"[{"Identifier":123,"Full name":"Alice","Balance":50.25},{"Identifier":456,"Full name":"Bob","Balance":72}]"#;
    {
        let out = ok(&program, RECORDS, None);
        assert_eq!(out, format!("{expected}\n"));
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed[1]["Full name"], "Bob");
        assert_eq!(parsed[0]["Balance"], 50.25);
    }
    // The table result rendered by the host as JSON is the same document,
    // and as CSV the spec's bytes.
    let table = compile(&PROGRAM.replace("    csv csv-options\n", ""), "table.alc").unwrap();
    assert_eq!(table.output(), Output::TableRows);
    assert_eq!(
        ok(&table, RECORDS, Some(Renderer::Json)),
        format!("{expected}\n")
    );
    assert_eq!(ok(&table, RECORDS, Some(Renderer::Csv)), EXPECTED_CSV);
    assert_eq!(ok(&table, RECORDS, None), EXPECTED_CSV);
    // A renderer for a program that renders its own text is refused.
    let (fail, _) = drive(&program, RECORDS, Some(Renderer::Json), &Limits::default());
    let fail = fail.unwrap_err();
    assert_eq!(fail.code, Code::DslTypeError);
    assert!(fail.message.starts_with("render_of_text: "), "{fail}");
}

/// The output limit is the writer's: a run that would exceed it fails
/// with the limit named, and nothing past it is written.
#[test]
fn the_output_limit_is_enforced_by_the_writer() {
    let limits = Limits {
        max_output_bytes: Some(200),
        ..Limits::default()
    };
    let (fail, _) = err(&spec_program(), &support::records_json(50), &limits);
    assert_eq!(fail.code, Code::ResourceLimitExceeded);
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_output_bytes");
}

/// `sink_out` takes any text output: a writer with no budget commits
/// every fragment as it is written.
#[test]
fn sink_out_takes_the_hosts_own_text_output() {
    let program = compile(PROGRAM, "export.alc").unwrap();
    let buffer = Shared::default();
    let out = WriteOut::new(buffer.clone()).with_budget(0);
    let mut sink = program
        .sink_out(Box::new(out), None, &Limits::default(), Metrics::new())
        .unwrap();
    let (outcome, events) =
        ParserSource::new(tabnas_json::make(), RECORDS).run_owned(Vec::<OwnedJsonEvent>::new());
    outcome.unwrap();
    replay(&events, &mut sink).unwrap();
    assert_eq!(
        String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap(),
        EXPECTED_CSV
    );
}

/// Run `work` on a thread of the stack the evaluator's bound is promised,
/// as the command and a host's run thread give it.
fn on_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(tabnas_alchemy::STACK_BYTES)
            .spawn_scoped(scope, work)
            .unwrap()
            .join()
            .unwrap()
    })
}

/// Definitions `a0` to `a{levels}`, each the concatenation of the one
/// before it with itself: a text of 10 * 2^levels bytes, built of shared
/// parts in linear time.
fn doubling(levels: usize) -> String {
    let mut src = String::from("def a0 \"0123456789\"\n");
    for i in 1..=levels {
        src.push_str(&format!("def a{i} (concat a{} a{})\n", i - 1, i - 1));
    }
    src
}

/// A program is code, and code can ask for too much before it reads a
/// byte. Building the plan is bounded: its steps by `max_plan_steps`, its
/// nesting by the evaluation depth (a function applied to itself is
/// `recursion`, not a stack overflow), and a `concat` over shared parts
/// knows which item is live without walking them again.
#[test]
fn building_the_plan_is_bounded() {
    let d = format!("{}id{}", "(d ".repeat(40), ")".repeat(40));
    let expo = format!(
        "def id [x] x\ndef d [g] (fn [x] (g (g x)))\ndef export [input]\n  let [y ({d} 1)]\n    json input\n"
    );
    let fail = compile(&expo, "expo.alc").unwrap_err();
    assert_eq!(fail.code, Code::ResourceLimitExceeded, "{fail}");
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_plan_steps");
    let mut wide = String::from("def v0 [1 2 3]\n");
    for i in 1..=40 {
        wide.push_str(&format!("def v{i} (vector v{} v{})\n", i - 1, i - 1));
    }
    wide.push_str("def export [input] (concat (scalar-text csv-options v40) (json input))\n");
    let fail = compile(&wide, "wide.alc").unwrap_err();
    assert_eq!(fail.code, Code::ResourceLimitExceeded, "{fail}");
    let omega = "def w [f] (f f)\ndef export [input]\n  let [x (w w)]\n    json input\n";
    let fail = compile(omega, "omega.alc").unwrap_err();
    assert_eq!(fail.code, Code::StreamabilityUnknown, "{fail}");
    assert!(fail.message.starts_with("recursion: "), "{fail}");
    assert_eq!((fail.row, fail.column), (Some(1), Some(12)));
    let start = Instant::now();
    let shared = doubling(40) + "def export [input] (concat a40 (json input))\n";
    let program = compile(&shared, "shared.alc").unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
    // Its prefix is 10 TB; the output limit ends it.
    let limits = Limits {
        max_output_bytes: Some(1000),
        ..Limits::default()
    };
    let (fail, out) = err(&program, "1", &limits);
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_output_bytes");
    assert!(out.len() <= 1000, "{}", out.len());
}

/// A function applied to itself per item fails with `recursion` at the
/// item, on the run thread, rather than aborting the host.
#[test]
fn self_application_per_item_is_recursion_at_run_time() {
    let src = "def w [f] (f f)\ndef export [input]\n  pipe input\n    select (path each-index)\n    map (fn [x] (w w))\n    join \",\"\n";
    let program = compile(src, "omega.alc").unwrap();
    let (fail, out) = on_stack(|| err(&program, "[1]", &Limits::default()));
    assert_eq!(fail.code, Code::StreamabilityUnknown, "{fail}");
    assert!(fail.message.starts_with("recursion: "), "{fail}");
    assert_eq!(out, "");
}

/// The host's abort flag reaches the program's own functions: a long
/// computation on one item stops with `ABORTED` at the next evaluation
/// step, not when the item is done. The source is not given the flag
/// here, so only the program can have seen it.
#[test]
fn the_abort_flag_stops_a_long_computation_on_one_item() {
    let d = format!("{}id{}", "(d ".repeat(30), ")".repeat(30));
    let src = format!(
        "def id [x] x\ndef d [g] (fn [x] (g (g x)))\ndef export [input]\n  join \",\"\n    map (fn [x] ({d} x)) (select (path each-index) input)\n"
    );
    let flag = AbortFlag::new();
    let program = compile(&src, "long.alc").unwrap().with_abort(flag.clone());
    let (outcome, events) =
        ParserSource::new(tabnas_json::make(), "[1,2,3]").run_owned(Vec::<OwnedJsonEvent>::new());
    outcome.unwrap();
    let mut sink = program
        .sink(
            Box::new(Shared::default()),
            None,
            &Limits::default(),
            Metrics::new(),
        )
        .unwrap();
    flag.abort();
    let start = Instant::now();
    let fail = replay(&events, &mut sink).unwrap_err();
    assert_eq!(fail.code, Code::Aborted, "{fail}");
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

/// The output limit bounds what a finite text holds, not only what is
/// written: `replace-text` over a finite text streams through the
/// replacer (it holds at most the literal), so the limit stops it at its
/// first byte past; a `join` or `concat-map` item is assembled whole to
/// write it atomically, and the assembly fails as soon as it passes the
/// limit, before anything of it is written.
#[test]
fn the_output_limit_bounds_a_finite_text() {
    let limits = Limits {
        max_output_bytes: Some(100_000),
        ..Limits::default()
    };
    let replace = doubling(25) + "def export [input] (replace-text \"0\" \"x\" a25)\n";
    let program = compile(&replace, "replace.alc").unwrap();
    let (fail, out) = err(&program, "1", &limits);
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_output_bytes");
    // The writer refuses the fragment that would cross the limit, so what
    // reached it is the text up to there.
    assert!(fail.committed_output, "{fail}");
    assert!(out.len() > 50_000 && out.len() <= 100_000, "{}", out.len());
    assert!(out.starts_with("x123456789x123456789"), "{}", &out[..20]);
    for step in ["join \",\"", "concat-map (fn [t] t)"] {
        let src = doubling(25)
            + &format!(
                "def export [input] ({step} (map (fn [x] a25) (select (path each-index) input)))\n"
            );
        let program = compile(&src, "items.alc").unwrap();
        let (fail, out) = err(&program, "[1,2]", &limits);
        assert_eq!(
            fail.limit.as_ref().unwrap().name,
            "max_output_bytes",
            "{step}"
        );
        assert_eq!(out, "", "{step}");
    }
}

/// A `scan-emit` state is what a stage retains from item to item, so it is
/// measured as it changes: no deeper than `max_depth` (a state that wraps
/// itself once per item fails at the item that passes it, rather than
/// being dropped one level inside the next on a stack that has a bottom),
/// no larger than `max_metadata_bytes`, and reported in
/// `retained_bytes_high`.
#[test]
fn a_scan_emit_state_is_measured_and_capped() {
    let grow = "def step [s x] (transition [s x] [])\ndef fin [s] [\"done\"]\ndef export [input]\n  join \",\"\n    scan-emit null step fin (select (path each-index) input)\n";
    let program = compile(grow, "grow.alc").unwrap();
    let numbers = format!("[{}]", vec!["1"; 300].join(","));
    let (fail, _) = err(&program, &numbers, &Limits::default());
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_depth", "{fail}");
    assert_eq!((fail.row, fail.column), (Some(5), Some(5)));
    // The same state under a small byte limit.
    let small = Limits {
        max_metadata_bytes: 256,
        ..Limits::default()
    };
    let (fail, _) = err(&program, &numbers, &small);
    assert_eq!(
        fail.limit.as_ref().unwrap().name,
        "max_metadata_bytes",
        "{fail}"
    );
    // A state that wraps itself in a partial once per item holds the
    // one before it inside the function, not the arguments: measured link
    // by link, it fails as the vector does.
    let chain = "def step [s x] (transition (partial s x) [])\ndef fin [s] [\"done\"]\ndef export [input]\n  join \",\"\n    scan-emit (fn [a] a) step fin (select (path each-index) input)\n";
    let program = compile(chain, "chain.alc").unwrap();
    let (fail, _) = err(&program, &numbers, &Limits::default());
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_depth", "{fail}");
    assert_eq!((fail.row, fail.column), (Some(5), Some(5)));
    let strings = format!(
        "[{}]",
        vec![format!("\"{}\"", "a".repeat(100)); 200].join(",")
    );
    let (fail, _) = err(&program, &strings, &small);
    assert_eq!(
        fail.limit.as_ref().unwrap().name,
        "max_metadata_bytes",
        "{fail}"
    );
    // A state that keeps the one before inside the partial a finite text's
    // concat-map applies is walked through that function: it fails as the
    // vector does, rather than growing a level per item.
    let text = "def g [prev y] y\ndef step [s x] (transition [(concat-map (partial g s) [\"x\"])] [])\ndef fin [s] [\"done\"]\ndef export [input]\n  join \",\"\n    scan-emit [] step fin (select (path each-index) input)\n";
    let program = compile(text, "text.alc").unwrap();
    let (fail, _) = err(&program, &numbers, &Limits::default());
    assert_eq!(fail.limit.as_ref().unwrap().name, "max_depth", "{fail}");
    assert_eq!((fail.row, fail.column), (Some(6), Some(5)));
    let (fail, _) = err(&program, &numbers, &small);
    assert_eq!(
        fail.limit.as_ref().unwrap().name,
        "max_metadata_bytes",
        "{fail}"
    );
    // A state that keeps the last item is measured and reported.
    let last = "def step [s x] (transition x [])\ndef fin [s] [\"done\"]\ndef export [input]\n  join \",\"\n    scan-emit null step fin (select (path each-index) input)\n";
    let program = compile(last, "last.alc").unwrap();
    let metrics = Metrics::new();
    let buffer = Shared::default();
    let sink = program
        .sink(
            Box::new(buffer.clone()),
            None,
            &Limits::default(),
            metrics.clone(),
        )
        .unwrap();
    let doc = format!(r#"[{{"k":"{}"}},{{"k":"b"}}]"#, "a".repeat(500));
    let (outcome, _) = ParserSource::new(tabnas_json::make(), &doc)
        .metrics(metrics.clone())
        .run_owned(sink);
    outcome.unwrap();
    assert_eq!(
        String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap(),
        "done"
    );
    assert!(Metrics::get(&metrics.retained_bytes_high) >= 500);
}

/// The initial state is retained like any other: measured when the stage
/// is built, so neither a step that hands the same state back nor a source
/// with no items carries one past the limits, and the failure comes before
/// anything is read or written.
#[test]
fn a_scan_emit_initial_state_is_measured_and_capped() {
    let tail = "def fin [s] [\"done\"]\ndef export [input]\n  join \",\"\n    scan-emit init step fin (select (path each-index) input)\n";
    let keep = "def step [s x] (transition s [])\n";
    let big = format!("def init [\"{}\"]\n{keep}{tail}", "a".repeat(1000));
    let program = compile(&big, "big.alc").unwrap();
    let small = Limits {
        max_metadata_bytes: 256,
        ..Limits::default()
    };
    for input in ["[1,2,3]", "[]"] {
        let (fail, out) = err(&program, input, &small);
        assert_eq!(
            fail.limit.as_ref().unwrap().name,
            "max_metadata_bytes",
            "{input}: {fail}"
        );
        assert_eq!((fail.row, fail.column), (Some(6), Some(5)), "{input}");
        assert_eq!(out, "", "{input}");
        assert!(!fail.committed_output, "{input}");
    }
    let deep = format!(
        "def init {}1{}\n{keep}{tail}",
        "[".repeat(10),
        "]".repeat(10)
    );
    let program = compile(&deep, "deep.alc").unwrap();
    let shallow = Limits {
        max_depth: 8,
        ..Limits::default()
    };
    for input in ["[1,2,3]", "[]"] {
        let (fail, out) = err(&program, input, &shallow);
        assert_eq!(
            fail.limit.as_ref().unwrap().name,
            "max_depth",
            "{input}: {fail}"
        );
        assert_eq!(out, "", "{input}");
    }
    // Under the defaults it runs, and the state it retains is reported.
    let program = compile(&big, "big.alc").unwrap();
    for input in ["[1,2,3]", "[]"] {
        let metrics = Metrics::new();
        let buffer = Shared::default();
        let sink = program
            .sink(
                Box::new(buffer.clone()),
                None,
                &Limits::default(),
                metrics.clone(),
            )
            .unwrap();
        let (outcome, _) = ParserSource::new(tabnas_json::make(), input)
            .metrics(metrics.clone())
            .run_owned(sink);
        outcome.unwrap();
        assert_eq!(
            String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap(),
            "done",
            "{input}"
        );
        assert!(
            Metrics::get(&metrics.retained_bytes_high) >= 1000,
            "{input}: {}",
            Metrics::get(&metrics.retained_bytes_high)
        );
    }
}

/// The source of the record-end runs: an array of `n` small objects, then
/// one cut off inside a member, which the json grammar refuses. The whole
/// elements are past the writer's 32 KiB budget, so it drains part of the
/// text before the failure.
fn cut_array(n: usize) -> String {
    let items: Vec<String> = (0..n)
        .map(|i| format!(r#"{{"id":{i},"name":"item {i}"}}"#))
        .collect();
    format!(r#"[{},{{"id":{n},"name":}}]"#, items.join(","))
}

/// tabnas/alchemy#29: `json` over a program's events marks each value
/// directly inside the root, and the writer holds a value until its end,
/// so a run that fails inside one leaves the values before it whole.
#[test]
fn a_failed_json_run_leaves_whole_values() {
    let program = compile("def export [input] (json input)", "export.alc").unwrap();
    let (outcome, out) = drive(&program, &cut_array(4000), None, &Limits::default());
    let fail = outcome.expect_err("the source fails");
    assert_eq!(fail.code, Code::InputInvalid, "{fail}");
    assert!(fail.committed_output, "{fail}");
    let n = out.matches(r#""id":"#).count();
    assert!(n > 0 && n < 4000, "{n} values written");
    let whole: Vec<String> = (0..n)
        .map(|i| format!(r#"{{"id":{i},"name":"item {i}"}}"#))
        .collect();
    assert_eq!(out, format!("[{}", whole.join(",")));
}

/// tabnas/alchemy#29: a step of `scan-emit` answers `:record-end` where a
/// record of its text ends, `join` writes it as no item, and the writer
/// holds a record until its end, so a run that fails inside one leaves the
/// records before it whole. Without the marks
/// the writer drains at its budget, wherever that falls.
#[test]
fn a_failed_scan_emit_run_leaves_whole_records() {
    let source = r#"def step [s event]
  match event
    case array-start (transition (push :array s) [])
    case array-end (transition (pop s) [])
    case object-start (transition (push :object s) [])
    case object-end
      match (count s)
        case 2 (transition (pop s) [:record-end])
        case _ (transition (pop s) [])
    case (key name) (transition s [(quoted name) ": "])
    case (scalar value) (transition s [(scalar-text csv-options value) "\n"])

def export [input]
  join ""
    scan-emit [] step (fn [s] []) (events input)
"#;
    let program = compile(source, "lines.alc").unwrap();
    {
        let (outcome, out) = drive(&program, &cut_array(4000), None, &Limits::default());
        let fail = outcome.expect_err("the source fails");
        assert_eq!(fail.code, Code::InputInvalid, "{fail}");
        assert!(fail.committed_output, "{fail}");
        let n = out.matches("\"id\": ").count();
        assert!(n > 0 && n < 4000, "{n} records written");
        let whole = (0..n)
            .map(|i| format!("\"id\": {i}\n\"name\": item {i}\n"))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(out, whole);
    }
    // A run that does not fail writes the same text as one without marks.
    let program = compile(source, "lines.alc").unwrap();
    assert_eq!(
        ok(&program, r#"[{"a":1},{"b":"x"}]"#, None),
        "\"a\": 1\n\"b\": x\n"
    );
}

/// tabnas/alchemy#29: `concat-map` marks the end of each item it writes,
/// and `replace-text`, which writes an item in several fragments, passes
/// the marks on, so a run that fails part way leaves whole items. Each
/// item is longer than the writer's budget, so without the marks its last
/// fragment is still buffered when the failure drops the buffer.
#[test]
fn a_failed_concat_map_run_through_replace_text_leaves_whole_items() {
    let source = r#"def line [x]
  concat (get "name" x) "\n"

def export [input]
  replace-text "\n" "\r\n"
    concat-map line (select (path each-index) input)
"#;
    let program = compile(source, "lines.alc").unwrap();
    let name = |i: usize| format!("{i}{}", "x".repeat(40 * 1024));
    let items: Vec<String> = (0..3)
        .map(|i| format!(r#"{{"name":"{}"}}"#, name(i)))
        .collect();
    let text = format!(r#"[{},{{"name":}}]"#, items.join(","));
    let (outcome, out) = drive(&program, &text, None, &Limits::default());
    let fail = outcome.expect_err("the source fails");
    assert_eq!(fail.code, Code::InputInvalid, "{fail}");
    assert!(fail.committed_output, "{fail}");
    let whole = (0..3)
        .map(|i| format!("{}\r\n", name(i)))
        .collect::<Vec<_>>()
        .concat();
    assert!(
        out == whole,
        "{} bytes written, not the 3 whole items",
        out.len()
    );
}
