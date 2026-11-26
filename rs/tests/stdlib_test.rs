// The standard library's text, run on transduce's routers and render's
// renderers: the one implementation of `table-from-json`, `csv`,
// `transduce-tree` and `transduce-records` (the ruling of 2026-10-24:
// nothing native stands in for the text), over every transduce fixture a
// grammar these tests can read, the generated documents transduce's tests
// and benches share, and the documents below. Where an oracle exists the
// text is held to it: the spec's worked-example program writes the bytes
// transduce's table transducer and render's CSV renderer write for the
// same binding, or fails with their code; a tree-shaped part writes the
// walk of the grammar's own value, as `transduce.tsv`'s rule says; and the
// library's `csv` refuses what the renderer refuses. That the library
// loads, which runs nothing, stays in alchemy's stdlib_test.rs.

mod common;
#[path = "../../test/fixtures/transduce/rs/tests/support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tabnas::Tabnas;
use tabnas_alchemy::shared::{
    replay, walk_datum, Code, Datum, DatumBuilder, Duplicates, Fail, Flow, JsonEvent, JsonOptions,
    Limits, Metrics, Number, OwnedJsonEvent, Sink,
};
use tabnas_alchemy::stdlib::registry::shortest_number;
use tabnas_alchemy::{Program, Renderer};
use tabnas_render::{CsvOptions, CsvRenderer, JsonRenderer, StringOut, WriteOut};
use tabnas_transduce::{
    column_from_meta, AbortFlag, Guarded, ParserSource, Schema, Selector, TableBinding,
    TableFromJson,
};

use common::json_into;

use common::compile;

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

/// A writer the test keeps a handle on after the sink took it.
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

/// The grammar for a fixture, by extension, among those this crate's
/// tests take; `None` skips the fixture.
fn grammar_for(path: &Path) -> Option<fn() -> Tabnas> {
    match path.extension().and_then(|e| e.to_str())? {
        "json" => Some(tabnas_json::make),
        "jsonl" => Some(tabnas_jsonl::make),
        "yaml" => Some(tabnas_yaml::make),
        "csv" | "tsv" => Some(tabnas_csv::make),
        _ => None,
    }
}

/// The events of one document, recorded once so both paths replay the
/// same stream (the walk of the grammar's value, sound for every grammar).
fn events(make: fn() -> Tabnas, text: &str) -> Result<Vec<OwnedJsonEvent>, Fail> {
    let (outcome, events) = ParserSource::new(make(), text).run_owned(Vec::new());
    outcome.map(|_| events)
}

/// Replay `events` through `program`; the output, or the failure.
fn run(
    program: &Program,
    events: &[OwnedJsonEvent],
    render: Option<Renderer>,
) -> Result<String, Fail> {
    run_under(program, events, render, &Limits::default(), Metrics::new())
}

/// [`run`] under the host's `limits`, counting in `metrics`.
fn run_under(
    program: &Program,
    events: &[OwnedJsonEvent],
    render: Option<Renderer>,
    limits: &Limits,
    metrics: Arc<Metrics>,
) -> Result<String, Fail> {
    let buffer = Shared::default();
    let mut sink = program.sink(Box::new(buffer.clone()), render, limits, metrics)?;
    replay(events, &mut sink)?;
    let bytes = buffer.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("utf-8 output"))
}

/// Where the worked example's records are.
fn records_selector() -> Selector {
    Selector::root()
        .property("response")
        .property("payload")
        .property("deep")
        .property("records")
        .each_index()
}

/// The components the library's text is held to: transduce's table
/// transducer over the worked example's binding (its columns from the
/// metadata, or inferred from the first row) and render's CSV renderer in
/// its default dialect, the events replayed through the source's guard;
/// the bytes, or the failure.
fn components(events: &[OwnedJsonEvent], limits: &Limits, infer: bool) -> Result<String, Fail> {
    let buffer = Shared::default();
    let csv = CsvRenderer::new(WriteOut::new(buffer.clone()), CsvOptions::default())?;
    let schema = if infer {
        Schema::Infer
    } else {
        Schema::FromMetadata {
            columns: Selector::root()
                .property("response")
                .property("metadata")
                .property("fields"),
            column: Box::new(column_from_meta),
        }
    };
    let table = TableFromJson::new(
        TableBinding {
            schema,
            rows: records_selector(),
        },
        limits,
        Duplicates::Reject,
        Metrics::new(),
        csv,
    )?;
    let mut guarded = Guarded::new(table, limits, AbortFlag::new(), Metrics::new());
    replay(events, &mut guarded)?;
    let bytes = buffer.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("utf-8 output"))
}

/// One document through the library's text and through the components:
/// the same bytes, or the same code.
fn agrees_with_components(
    name: &str,
    program: &Program,
    events: &[OwnedJsonEvent],
    infer: bool,
) -> Result<(), String> {
    match (
        run(program, events, None),
        components(events, &Limits::default(), infer),
    ) {
        (Ok(a), Ok(b)) if a == b => Ok(()),
        (Ok(a), Ok(b)) => Err(format!(
            "{name}: the bytes differ\n  text:       {a:?}\n  components: {b:?}"
        )),
        (Err(a), Err(b)) if a.code == b.code => Ok(()),
        (Err(a), Err(b)) => Err(format!(
            "{name}: the codes differ\n  text:       {a}\n  components: {b}"
        )),
        (Ok(a), Err(b)) => Err(format!(
            "{name}: the text produced {a:?}, the components failed: {b}"
        )),
        (Err(a), Ok(b)) => Err(format!(
            "{name}: the components produced {b:?}, the text failed: {a}"
        )),
    }
}

/// A document to compare: its name, the grammar that reads it, its text.
type Document = (String, fn() -> Tabnas, String);

fn fixtures_dir() -> PathBuf {
    common::vendored().join("transduce/rs/tests/fixtures")
}

/// Every transduce fixture a grammar here reads, plus the generated
/// documents in every shape: the text writes the components' bytes, or
/// fails with their code.
#[test]
fn the_library_writes_what_the_components_write_for_every_fixture_and_generated_document() {
    let program = compile(PROGRAM, "export.alc").expect("the spec's program compiles");
    let mut documents: Vec<Document> = Vec::new();
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "transduce's fixtures are vendored, at {}: {e}",
                dir.display()
            )
        })
        .map(|e| e.expect("a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let Some(make) = grammar_for(&path) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("a fixture reads");
        documents.push((path.display().to_string(), make, text));
    }
    for n in [0, 1, 2, 3, 50] {
        documents.push((
            format!("records_json({n})"),
            tabnas_json::make,
            support::records_json(n),
        ));
    }
    documents.push((
        "records_yaml(3)".into(),
        tabnas_yaml::make,
        support::records_yaml(3),
    ));
    documents.push((
        "records_jsonl(3)".into(),
        tabnas_jsonl::make,
        support::records_jsonl(3),
    ));
    documents.push((
        "records_csv(3)".into(),
        tabnas_csv::make,
        support::records_csv(3),
    ));
    // Cells of every kind, in the worked example's shape: null, missing
    // members (a failure under the standard options), nested containers,
    // booleans, quotes, delimiters, line breaks and non-ASCII text.
    let meta = support::METADATA;
    let shaped = |records: &str| {
        format!(
            r#"{{"response":{{"metadata":{meta},"payload":{{"deep":{{"records":[{records}]}}}}}}}}"#
        )
    };
    for (name, records) in [
        (
            "nulls",
            r#"{"id":null,"person":{"name":null},"account":{"balance":null}}"#,
        ),
        ("missing", r#"{"id":1,"person":{},"account":{"balance":2}}"#),
        (
            "containers",
            r#"{"id":[1,2.50],"person":{"name":{"first":"A","last":"B"}},"account":{"balance":{}}}"#,
        ),
        (
            "booleans",
            r#"{"id":true,"person":{"name":false},"account":{"balance":0}}"#,
        ),
        (
            "quotes",
            r#"{"id":"say \"hi\"","person":{"name":"a,b"},"account":{"balance":"line\r\nbreak"}}"#,
        ),
        (
            "unicode",
            r#"{"id":"caf\u00e9","person":{"name":"日本"},"account":{"balance":"\ud83d\ude00"}}"#,
        ),
        (
            "lexemes",
            r#"{"id":1e2,"person":{"name":"x"},"account":{"balance":-0.0}}"#,
        ),
        (
            "big",
            r#"{"id":12345678901234567890123,"person":{"name":"x"},"account":{"balance":1E+2}}"#,
        ),
        (
            "two rows out of order",
            r#"{"account":{"balance":1},"id":2,"person":{"name":"z"}},{"id":3,"person":{"name":"y"},"account":{"balance":4}}"#,
        ),
    ] {
        documents.push((name.into(), tabnas_json::make, shaped(records)));
    }
    // The order contract and the absent shapes.
    documents.push((
        "metadata after rows".into(),
        tabnas_json::make,
        format!(
            r#"{{"response":{{"payload":{{"deep":{{"records":[{}]}}}},"metadata":{meta}}}}}"#,
            support::record(1)
        ),
    ));
    documents.push((
        "no metadata".into(),
        tabnas_json::make,
        format!(
            r#"{{"response":{{"payload":{{"deep":{{"records":[{}]}}}}}}}}"#,
            support::record(1)
        ),
    ));
    documents.push((
        "no records".into(),
        tabnas_json::make,
        format!(r#"{{"response":{{"metadata":{meta}}}}}"#),
    ));
    documents.push((
        "metadata not an array".into(),
        tabnas_json::make,
        r#"{"response":{"metadata":{"fields":{}}}}"#.into(),
    ));
    documents.push((
        "descriptor with a bad segment".into(),
        tabnas_json::make,
        r#"{"response":{"metadata":{"fields":[{"title":"a","path":[true]}]}}}"#.into(),
    ));
    documents.push((
        "row that is not an object".into(),
        tabnas_json::make,
        shaped("1"),
    ));

    let total = documents.len();
    let mut failures = Vec::new();
    let mut skipped = 0;
    for (i, (name, make, text)) in documents.iter().enumerate() {
        if i % 10 == 0 {
            println!("sweep: {i} of {total} ({}%)", i * 100 / total);
        }
        let events = match events(*make, text) {
            Ok(events) => events,
            Err(_) => {
                // The grammar refused the document: nothing reached a
                // program, so there is nothing to compare.
                skipped += 1;
                continue;
            }
        };
        if let Err(report) = agrees_with_components(name, &program, &events, false) {
            failures.push(report);
        }
    }
    println!("sweep: {total} of {total} (100%), {skipped} unreadable");
    assert!(total - skipped > 20, "enough documents were read");
    assert!(
        failures.is_empty(),
        "{} document(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The other renderings the text offers: the table as records, rendered
/// as JSON (`records` then `json`), and a `csv` whose options record
/// names another dialect, which the text writes as named.
#[test]
fn the_other_renderings_write_as_their_programs_say() {
    let records = PROGRAM.replace("    csv csv-options\n", "    records\n    json\n");
    let program = compile(&records, "records.alc").unwrap();
    let dialect = PROGRAM.replace(
        "    csv csv-options\n",
        "    csv options\n\ndef options\n  record\n    entry :delimiter \";\"\n    entry :newline \"\\n\"\n    entry :header false\n    entry :null-text \"NULL\"\n    entry :missing \"-\"\n",
    );
    let dialect = compile(&dialect, "dialect.alc").unwrap();
    let meta = support::METADATA;
    let three = events(tabnas_json::make, &support::records_json(3)).unwrap();
    let out = run(&program, &three, None).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("records render as JSON");
    let rows = parsed.as_array().expect("an array of records");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["Full name"], "Person number 2");
    let csv = run(&dialect, &three, None).unwrap();
    assert_eq!(csv.lines().count(), 3, "{csv}");
    assert!(csv.lines().all(|l| l.matches(';').count() == 2), "{csv}");
    let sparse = events(
        tabnas_json::make,
        &format!(
            r#"{{"response":{{"metadata":{meta},"payload":{{"deep":{{"records":[{{"id":null,"person":{{}},"account":{{"balance":"a;b"}}}}]}}}}}}}}"#
        ),
    )
    .unwrap();
    assert_eq!(
        run(&dialect, &sparse, None).unwrap(),
        "\"NULL\";\"-\";\"a;b\"\n"
    );
    // A missing cell is no member of the record.
    assert_eq!(
        run(&program, &sparse, None).unwrap(),
        "[{\"Identifier\":null,\"Balance\":\"a;b\"}]\n"
    );
}

/// The spec's worked example, byte for byte as aless's fixture has it.
const RECORDS: &str = r#"{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]},"payload":{"deep":{"records":[{"id":123,"person":{"name":"Alice"},"account":{"balance":50.25}},{"account":{"balance":72},"person":{"name":"Bob"},"id":456}]}}}}"#;

const EXPECTED_CSV: &str =
    "\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n";

/// One outcome: the bytes, or the code; `expected` is that outcome.
fn expect(
    name: &str,
    program: &Program,
    events: &[OwnedJsonEvent],
    limits: &Limits,
    expected: Result<&str, Code>,
) {
    let got = run_under(program, events, None, limits, Metrics::new());
    match (&got, expected) {
        (Ok(a), Ok(want)) => assert_eq!(a, want, "{name}"),
        (Err(a), Err(code)) => assert_eq!(a.code, code, "{name}: {a}"),
        _ => panic!("{name}: expected {expected:?}, got {got:?}"),
    }
}

/// The library's `csv` validates the table events it renders as render's
/// CSV renderer does (spec 13.2: the protocol validator is not omitted),
/// so a program that produces its own table events, or a document or an
/// options record the renderer refuses, fails with the renderer's code,
/// and a sound one prints the renderer's bytes.
#[test]
fn the_library_csv_validates_what_the_renderer_validates() {
    let items = events(
        tabnas_json::make,
        r#"{"items":[{"n":"a","v":1},{"n":"b\"q","v":-2},{"n":"c","v":null}],"tail":"t"}"#,
    )
    .unwrap();
    let limits = Limits::default();
    let label = |l: &str| format!("(record (entry :label \"{l}\"))");
    let table = |step: &str, finish: &str| {
        format!(
            "def export [input] (csv csv-options (scan-emit no-schema {step} {finish} (select (path \"items\" each-index) input)))"
        )
    };
    // One schema on the first item, a row per item: the sound shape.
    let first = |schema: &str, row: &str| {
        format!(
            "(fn [s x] (transition (ready []) (if (is-ready s) [(row {row})] [(schema {schema}) (row {row})])))"
        )
    };
    let n = label("N");
    let v = label("V");
    let cases: Vec<(&str, String, Result<&str, Code>)> = vec![
        (
            "sound",
            table(&first(&format!("[{n} {v}]"), "[(get :n x) (get :v x)]"), "(fn [s] [table-end])"),
            Ok("\"N\",\"V\"\r\n\"a\",\"1\"\r\n\"b\"\"q\",\"-2\"\r\n\"c\",\"\"\r\n"),
        ),
        (
            "rows without a schema",
            table("(fn [s x] (transition s [(row [(get :n x)])]))", "(fn [s] [table-end])"),
            Err(Code::ProtocolOrderError),
        ),
        (
            "a schema per item",
            table(&format!("(fn [s x] (transition s [(schema [{n}])]))"), "(fn [s] [table-end])"),
            Err(Code::ProtocolOrderError),
        ),
        (
            "a row wider than the schema",
            table(&first(&format!("[{n}]"), "[(get :n x) (get :v x)]"), "(fn [s] [table-end])"),
            Err(Code::ProtocolOrderError),
        ),
        (
            "no table-end",
            table(&first(&format!("[{n}]"), "[(get :n x)]"), "(fn [s] [])"),
            Err(Code::ProtocolOrderError),
        ),
        (
            "two table-ends",
            table(&first(&format!("[{n}]"), "[(get :n x)]"), "(fn [s] [table-end table-end])"),
            Err(Code::ProtocolOrderError),
        ),
        (
            "an item that is not a table event",
            "def export [input] (csv csv-options (map (fn [x] (get :n x)) (select (path \"items\" each-index) input)))".to_string(),
            Err(Code::ProtocolOrderError),
        ),
        (
            "no items at all",
            "def export [input] (csv csv-options (scan-emit no-schema (fn [s x] (transition s [x])) (fn [s] []) (select (path \"none\" each-index) input)))".to_string(),
            Err(Code::ProtocolOrderError),
        ),
        (
            "a schema of no columns",
            table(&first("[]", "[]"), "(fn [s] [table-end])"),
            Err(Code::TargetValueUnrepresentable),
        ),
        (
            "a column without a label",
            table(&first("[(record (entry :x \"N\"))]", "[(get :n x)]"), "(fn [s] [table-end])"),
            Err(Code::MissingValue),
        ),
        (
            "a label that is a record",
            table(&first("[(record (entry :label (record)))]", "[(get :n x)]"), "(fn [s] [table-end])"),
            Err(Code::InputInvalid),
        ),
    ];
    for (name, src, expected) in cases {
        let program = compile(&src, "table.alc").unwrap_or_else(|f| panic!("{name}: {f}"));
        expect(name, &program, &items, &limits, expected);
    }

    // The worked example over a document whose metadata is empty: a table
    // of no columns has no CSV form.
    let program = compile(PROGRAM, "export.alc").unwrap();
    let empty = events(
        tabnas_json::make,
        r#"{"response":{"metadata":{"fields":[]},"payload":{"deep":{"records":[{"id":1}]}}}}"#,
    )
    .unwrap();
    expect(
        "no columns",
        &program,
        &empty,
        &limits,
        Err(Code::TargetValueUnrepresentable),
    );

    // A delimiter no CSV reader could take is refused before anything
    // runs (`csv-table`, as the renderer refuses it).
    let records = events(tabnas_json::make, RECORDS).unwrap();
    for delimiter in ["\\\"", "\\n", "\\r"] {
        let src = PROGRAM.replace(
            "    csv csv-options\n",
            &format!("    csv (record (entry :delimiter \"{delimiter}\") (entry :newline \"\\r\\n\") (entry :header true) (entry :null-text \"\") (entry :missing :error))\n"),
        );
        let program = compile(&src, "delimiter.alc").unwrap();
        expect(
            &format!("delimiter {delimiter}"),
            &program,
            &records,
            &limits,
            Err(Code::TargetValueUnrepresentable),
        );
    }

    // A column function that answers something other than a record fails
    // as `get` does.
    let keyword = compile(
        "def b (record (entry :columns (path \"m\")) (entry :rows (path \"r\" each-index)) (entry :column (fn [d] :oops)))\ndef export [input] (csv csv-options (table-from-json b input))",
        "keyword.alc",
    )
    .unwrap();
    let doc = events(
        tabnas_json::make,
        r#"{"m":[{"title":"t","path":["a"]}],"r":[{"a":"x"}]}"#,
    )
    .unwrap();
    expect(
        "a column that is a keyword",
        &keyword,
        &doc,
        &limits,
        Err(Code::DslTypeError),
    );
}

/// The library's table holds the scopes it captures to the host's limits
/// (the metadata under `max_metadata_bytes`, each row under
/// `max_record_bytes`, at most `max_columns` columns), so under the host's
/// own limits it fails naming the limit, or prints, and counts its rows.
#[test]
fn the_limits_hold() {
    let program = compile(PROGRAM, "export.alc").unwrap();
    let records = events(tabnas_json::make, RECORDS).unwrap();
    let cases: Vec<(&str, Limits, Result<&str, Code>)> = vec![
        ("defaults", Limits::default(), Ok(EXPECTED_CSV)),
        (
            "max_record_bytes",
            Limits {
                max_record_bytes: 100,
                ..Limits::default()
            },
            Err(Code::ResourceLimitExceeded),
        ),
        (
            "max_metadata_bytes",
            Limits {
                max_metadata_bytes: 50,
                ..Limits::default()
            },
            Err(Code::ResourceLimitExceeded),
        ),
        (
            "max_columns",
            Limits {
                max_columns: 2,
                ..Limits::default()
            },
            Err(Code::ResourceLimitExceeded),
        ),
        (
            "max_capture_bytes decides neither table",
            Limits {
                max_capture_bytes: 100,
                ..Limits::default()
            },
            Ok(EXPECTED_CSV),
        ),
    ];
    for (name, limits, expected) in cases {
        expect(name, &program, &records, &limits, expected);
    }
    // A cell that is a vector is its compact JSON text, one scalar of the
    // output, held to max_scalar_bytes.
    let containers = events(
        tabnas_json::make,
        &RECORDS.replace(r#""id":123"#, r#""id":[1,2,3,4,5,6]"#),
    )
    .unwrap();
    expect(
        "a container cell under max_scalar_bytes",
        &program,
        &containers,
        &Limits {
            max_scalar_bytes: 12,
            ..Limits::default()
        },
        Err(Code::ResourceLimitExceeded),
    );
    expect(
        "a container cell within it",
        &program,
        &containers,
        &Limits { max_scalar_bytes: 13, ..Limits::default() },
        Ok("\"Identifier\",\"Full name\",\"Balance\"\r\n\"[1,2,3,4,5,6]\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n"),
    );
    let metrics = Metrics::new();
    run_under(
        &program,
        &records,
        None,
        &Limits::default(),
        metrics.clone(),
    )
    .unwrap();
    assert_eq!(Metrics::get(&metrics.rows), 2);
}

/// A program over an inferred binding (`:columns :infer`): a root array
/// of rows, written as CSV with a missing cell as an empty field.
const INFERRED: &str = "def options
  record
    entry :delimiter \",\"
    entry :newline \"\\r\\n\"
    entry :header true
    entry :null-text \"\"
    entry :missing \"\"

def rows-binding
  record
    entry :columns :infer
    entry :rows (path each-index)

def export [input]
  pipe input
    table-from-json rows-binding
    csv options
";

/// The inferred binding: the columns are the first row's keys, in its
/// order, each reading its own key; the library's text infers them with
/// `keys`, over every shape a first row and a later row can take, and
/// holds them to the limits. Over the worked example's documents it writes
/// what transduce's `Schema::Infer` and render's CSV renderer write.
#[test]
fn an_inferred_binding_infers_the_first_rows_columns() {
    let program = compile(INFERRED, "inferred.alc").expect("the inferred program compiles");
    let json = |text: &str| events(tabnas_json::make, text).unwrap();
    let ok = |name: &str, text: &str, want: &str| {
        expect(name, &program, &json(text), &Limits::default(), Ok(want))
    };
    let err = |name: &str, text: &str, code: Code| {
        expect(name, &program, &json(text), &Limits::default(), Err(code))
    };
    ok(
        "the first row's keys, in its order",
        r#"[{"b":1,"a":"x"},{"a":"y","b":2}]"#,
        "\"b\",\"a\"\r\n\"1\",\"x\"\r\n\"2\",\"y\"\r\n",
    );
    ok(
        "a key a later row lacks is a missing cell",
        r#"[{"a":1,"b":2},{"a":3}]"#,
        "\"a\",\"b\"\r\n\"1\",\"2\"\r\n\"3\",\"\"\r\n",
    );
    ok(
        "a key only a later row has is no column",
        r#"[{"a":1},{"c":3,"a":2}]"#,
        "\"a\"\r\n\"1\"\r\n\"2\"\r\n",
    );
    ok(
        "cells of every kind",
        r#"[{"s":"q\"x","n":1.5,"t":true,"z":null,"o":{"k":[1,2]}}]"#,
        "\"s\",\"n\",\"t\",\"z\",\"o\"\r\n\"q\"\"x\",\"1.5\",\"true\",\"\",\"{\"\"k\"\":[1,2]}\"\r\n",
    );
    ok(
        "a key that spells an index names a member",
        r#"[{"0":"zero","1":"one"}]"#,
        "\"0\",\"1\"\r\n\"zero\",\"one\"\r\n",
    );
    ok(
        "a later row that is not an object has missing cells",
        r#"[{"a":1},2]"#,
        "\"a\"\r\n\"1\"\r\n\"\"\r\n",
    );
    // A first row of another kind than an object: a scalar is one
    // `value` column, the row itself; an array's columns are its
    // positions. A later row projects through the first row's paths, so
    // an object under positional columns is a missing cell, and under a
    // `value` column its compact JSON text.
    ok(
        "a first row that is a scalar",
        r#"[1,{"a":1}]"#,
        "\"value\"\r\n\"1\"\r\n\"{\"\"a\"\":1}\"\r\n",
    );
    ok(
        "a first row that is an array",
        r#"[[1,"x"],{"a":1},[2]]"#,
        "\"0\",\"1\"\r\n\"1\",\"x\"\r\n\"\",\"\"\r\n\"2\",\"\"\r\n",
    );
    // A table of no columns, from no rows or from an empty first row, is
    // one the CSV renderer refuses, as it refuses any.
    err("no rows", "[]", Code::TargetValueUnrepresentable);
    err(
        "an empty first row",
        r#"[{},{"a":1}]"#,
        Code::TargetValueUnrepresentable,
    );
    let three = json(r#"[{"a":1,"b":2,"c":3}]"#);
    expect(
        "max_columns holds the inferred columns",
        &program,
        &three,
        &Limits {
            max_columns: 2,
            ..Limits::default()
        },
        Err(Code::ResourceLimitExceeded),
    );
    // Three one-byte names take 16 + 3 * (16 + 1) = 67 bytes as the
    // labels' array; the library's state holding them is larger still.
    expect(
        "max_metadata_bytes holds the inferred columns",
        &program,
        &three,
        &Limits {
            max_metadata_bytes: 40,
            ..Limits::default()
        },
        Err(Code::ResourceLimitExceeded),
    );
    expect(
        "max_record_bytes holds each row",
        &program,
        &three,
        &Limits {
            max_record_bytes: 8,
            ..Limits::default()
        },
        Err(Code::ResourceLimitExceeded),
    );
    // The column count holds where the schema is built, so it holds when
    // the table events reach no renderer that would check them.
    for (name, binding) in [
        ("an inferred table's events as items", "(record (entry :columns :infer) (entry :rows (path each-index)))"),
        ("a described table's events as items", "(record (entry :columns (path \"m\")) (entry :rows (path \"r\" each-index)) (entry :column (fn [d] (record (entry :label d) (entry :source (path d))))))"),
    ] {
        let items = compile(
            &format!("def b {binding}\ndef export [input] (join \"\" (map (fn [e] \"x\") (table-from-json b input)))"),
            "items.alc",
        )
        .unwrap();
        let events = if name.starts_with("an inferred") {
            three.clone()
        } else {
            json(r#"{"m":["a","b","c"],"r":[{"a":1,"b":2,"c":3}]}"#)
        };
        expect(
            name,
            &items,
            &events,
            &Limits {
                max_columns: 2,
                ..Limits::default()
            },
            Err(Code::ResourceLimitExceeded),
        );
        expect(name, &items, &events, &Limits::default(), Ok("xxx"));
    }
    // The worked example's documents, bound by inference from their rows.
    let api = INFERRED.replace(
        "(path each-index)",
        "(path \"response\" \"payload\" \"deep\" \"records\" each-index)",
    );
    let api = compile(&api, "inferred-api.alc").expect("the api-shaped program compiles");
    let mut failures = Vec::new();
    for n in [0, 1, 2, 3, 50] {
        let events = events(tabnas_json::make, &support::records_json(n)).unwrap();
        if let Err(report) =
            agrees_with_components(&format!("records_json({n})"), &api, &events, true)
        {
            failures.push(report);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each row counts once in `metrics.rows`, by the last table stage it
/// passes: the adapter to a renderer, or a `csv-table` whose rows reach
/// no later table stage. A `map`, a `filter` or a `scan-emit` between two table
/// stages hands the count on, so a row a filter drops is never counted,
/// and rows that only ever become a text of items are not rows of any
/// table.
#[test]
fn each_row_counts_once_across_composed_table_stages() {
    let tail = |t: &str| PROGRAM.replace("    csv csv-options\n", t);
    let identity = "    map (fn [e] e)\n";
    let drop_rows = "    filter (fn [e] (match e (case (row cells) false) (case _ true)))\n";
    let pass_scan = "    scan-emit null (fn [s e] (transition s [e])) (fn [s] []) \n";
    let as_text = "    map (fn [e] \"x\")\n    join \",\"\n";
    let cases: Vec<(&str, String, Option<Renderer>, u64)> = vec![
        ("csv", tail("    csv csv-options\n"), None, 2),
        (
            "csv-table as the result",
            tail("    csv-table csv-options\n"),
            None,
            2,
        ),
        (
            "csv-table as the result, as json",
            tail("    csv-table csv-options\n"),
            Some(Renderer::Json),
            2,
        ),
        (
            "csv over csv-table",
            tail("    csv-table csv-options\n    csv csv-options\n"),
            None,
            2,
        ),
        (
            "records over csv-table",
            tail("    csv-table csv-options\n    records\n    json\n"),
            None,
            2,
        ),
        (
            "csv-table twice",
            tail("    csv-table csv-options\n    csv-table csv-options\n    csv csv-options\n"),
            None,
            2,
        ),
        (
            "the library csv with its own dialect",
            tail("    csv opts\n\ndef opts\n  record\n    entry :delimiter \"||\"\n    entry :newline \"\\r\\n\"\n    entry :header true\n    entry :null-text \"\"\n    entry :missing :error\n"),
            None,
            2,
        ),
        (
            "a map, then the program's csv",
            tail(&format!("{identity}    csv csv-options\n")),
            None,
            2,
        ),
        (
            "a map, csv-table, csv",
            tail(&format!("{identity}    csv-table csv-options\n    csv csv-options\n")),
            None,
            2,
        ),
        (
            "csv-table, a map, csv",
            tail(&format!("    csv-table csv-options\n{identity}    csv csv-options\n")),
            None,
            2,
        ),
        (
            "a map, the host's renderer",
            tail(identity),
            Some(Renderer::Csv),
            2,
        ),
        (
            "a map, the host's json",
            tail(identity),
            Some(Renderer::Json),
            2,
        ),
        (
            "a filter that keeps every row, the host's renderer",
            tail("    filter (fn [e] true)\n"),
            Some(Renderer::Csv),
            2,
        ),
        (
            "a scan that passes each event on, then csv",
            tail(&format!("{pass_scan}    csv csv-options\n")),
            None,
            2,
        ),
        (
            "a filter that drops every row, then csv",
            tail(&format!("{drop_rows}    csv csv-options\n")),
            None,
            0,
        ),
        (
            "a filter that drops every row, the host's renderer",
            tail(drop_rows),
            Some(Renderer::Csv),
            0,
        ),
        (
            "the table's events as a text: no table stage",
            tail(as_text),
            None,
            0,
        ),
        (
            "csv-table's events as a text",
            tail(&format!("    csv-table csv-options\n{as_text}")),
            None,
            2,
        ),
    ];
    let records = events(tabnas_json::make, RECORDS).unwrap();
    for (name, src, render, rows) in cases {
        let program = compile(&src, "rows.alc").unwrap();
        let metrics = Metrics::new();
        run_under(
            &program,
            &records,
            render,
            &Limits::default(),
            metrics.clone(),
        )
        .unwrap_or_else(|f| panic!("{name}: {f}"));
        assert_eq!(Metrics::get(&metrics.rows), rows, "{name}");
    }
}

/// Where the text and the components answer apart, pinned so a change to
/// either is seen. Metadata selected twice is the text's own refusal,
/// `fail`, which is INPUT_INVALID, where transduce's table transducer
/// calls the same document an order violation, a table having one schema.
/// A descriptor without a title is a column without a label to the text's
/// `column-from-meta`, which the table refuses as a missing value, where
/// transduce's `column_from_meta` refuses the descriptor itself. The
/// standard shapes reach neither. And a numeric title renders by its
/// lexeme, one label policy serving every table.
#[test]
fn where_the_text_and_the_components_answer_apart() {
    let twice = PROGRAM.replace(
        "      path \"response\" \"metadata\" \"fields\"\n",
        "      path \"response\" \"metadata\" each-index\n",
    );
    let program = compile(&twice, "twice.alc").unwrap();
    let doc = format!(
        r#"{{"response":{{"metadata":[{},{}],"payload":{{"deep":{{"records":[{}]}}}}}}}}"#,
        r#"[{"title":"a","path":["id"]}]"#,
        r#"[{"title":"b","path":["id"]}]"#,
        support::record(1)
    );
    let twice = events(tabnas_json::make, &doc).unwrap();
    let fail = run(&program, &twice, None).unwrap_err();
    assert_eq!(fail.code, Code::InputInvalid, "{fail}");
    assert_eq!(
        components(&twice, &Limits::default(), false)
            .unwrap_err()
            .code,
        Code::InputOrderViolation
    );
    let program = compile(PROGRAM, "export.alc").unwrap();
    // A descriptor without a title: the text's column-from-meta makes a
    // column whose label is missing, which the table refuses as a missing
    // value; transduce's column_from_meta refuses the descriptor itself.
    let untitled = events(
        tabnas_json::make,
        r#"{"response":{"metadata":{"fields":[{"path":["a"]}]}}}"#,
    )
    .unwrap();
    let fail = run(&program, &untitled, None).unwrap_err();
    assert_eq!(fail.code, Code::MissingValue, "{fail}");
    assert_eq!(
        components(&untitled, &Limits::default(), false)
            .unwrap_err()
            .code,
        Code::InputInvalid
    );
    let doc = format!(
        r#"{{"response":{{"metadata":{{"fields":[{{"title":42,"path":["id"]}}]}},"payload":{{"deep":{{"records":[{}]}}}}}}}}"#,
        support::record(1)
    );
    let numeric = events(tabnas_json::make, &doc).unwrap();
    expect(
        "a numeric title",
        &program,
        &numeric,
        &Limits::default(),
        Ok("\"42\"\r\n\"1\"\r\n"),
    );
}

/// The events end with `End`, as every source promises; a recording that
/// does not is a source defect, and the sink says so rather than dropping
/// the output silently: nothing is flushed.
#[test]
fn a_stream_without_end_writes_nothing() {
    let program = compile(PROGRAM, "export.alc").unwrap();
    let mut events = events(tabnas_json::make, &support::records_json(2)).unwrap();
    assert_eq!(events.pop(), Some(OwnedJsonEvent::End));
    let buffer = Shared::default();
    let mut sink = program
        .sink(
            Box::new(buffer.clone()),
            None,
            &Limits::default(),
            Metrics::new(),
        )
        .unwrap();
    replay(&events, &mut sink).unwrap();
    assert!(buffer.0.lock().unwrap().is_empty());
    // And the End then completes it.
    sink.event(JsonEvent::End).unwrap();
    assert!(!buffer.0.lock().unwrap().is_empty());
}

/// A number is read and written as the renderers read and write it: the
/// library's `scalar-text` holds a lexeme to the JSON number grammar
/// with the reader's own (`lex::is_json_number`), which accepts exactly
/// what the renderers' (render's `is_json_number`) does, and writes a
/// number without one as `shortest_number` lays it out, which is what
/// render writes for one (its `number::write_value`). That is what lets
/// the library's `csv` and the host's CSV renderer agree byte for byte.
#[test]
fn numbers_are_read_and_written_as_the_renderers_do() {
    // Every text of up to five characters over what a JSON number is
    // made of, and a character that is not.
    let alphabet = ['0', '1', '9', '-', '+', '.', 'e', 'E', 'x'];
    let mut texts = vec![String::new()];
    let mut last = vec![String::new()];
    for _ in 0..5 {
        last = last
            .iter()
            .flat_map(|t| alphabet.iter().map(move |c| format!("{t}{c}")))
            .collect();
        texts.extend(last.iter().cloned());
    }
    let mut accepted = 0;
    for text in &texts {
        let ours = tabnas_alchemy::lex::is_json_number(text);
        assert_eq!(ours, tabnas_render::is_json_number(text), "{text:?}");
        accepted += usize::from(ours);
    }
    assert!(accepted > 1000, "{accepted} of {} accepted", texts.len());
    // render's `write_value` is its own (`pub(crate)`), and its JSON
    // renderer writes that function's text, and nothing else, for a
    // document that is one number without a lexeme.
    for value in [
        0.0,
        -0.0,
        1.0,
        -1.0,
        50.25,
        72.0,
        1e-6,
        1e-7,
        1e20,
        1e21,
        -2.5e-8,
        1.5e300,
        123456789.0,
        0.1 + 0.2,
        f64::MAX,
        f64::MIN_POSITIVE,
        5e-324,
    ] {
        let mut json = JsonRenderer::new(StringOut::new(), JsonOptions::default());
        json.event(JsonEvent::Number(Number::new(value))).unwrap();
        json.event(JsonEvent::End).unwrap();
        assert_eq!(
            json.into_inner().into_string(),
            shortest_number(value),
            "{value:e}"
        );
    }
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

/// The rule of `transduce.tsv`, the oracle a part has: what a part wrote,
/// read back from its JSON, is the walk of the grammar's own value of the
/// document, a number's lexeme aside. A member the part streamed twice is
/// kept once by the value, so such an output has no oracle and passes.
fn is_the_walk(written: &str, value: &tabnas::Value) -> Result<(), String> {
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
    let (written, walked) = (
        plain_events(&read),
        plain_events(&Datum::from_tabnas(value)),
    );
    if written == walked {
        Ok(())
    } else {
        Err(format!(
            "the part's events are not the walk of the grammar's value\n  written: {written:?}\n  walked:  {walked:?}"
        ))
    }
}

/// A part over every document: where the grammar accepts the document and
/// `oracle` says the part writes its tree, the output is the walk of the
/// grammar's value; where the grammar refuses it, the part fails, with the
/// grammar's refusal or with a reason of its own it met first; and any
/// other failure is the part's own reason (a catalogue code, as every
/// failure's is). The failures, for the test to report.
fn part_over(
    spec_name: &str,
    program: &Program,
    make: fn() -> Tabnas,
    documents: &[(String, String)],
    oracle: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, text) in documents {
        let name = format!("{spec_name}, {name}");
        match (run_passes(program, make, text), make().parse(text)) {
            // The part fails: with the grammar's refusal, or with a reason of
            // its own it met before the refusal arrived.
            (Err(_), Err(_)) => {}
            (Err(fail), Ok(_)) if oracle => failures.push(format!("{name}: {fail}")),
            (Err(_), Ok(_)) => {}
            (Ok(written), Err(_)) => failures.push(format!(
                "{name}: wrote {written:?} of a document the grammar refuses"
            )),
            (Ok(written), Ok(value)) if oracle => {
                if let Err(report) = is_the_walk(&written, &value) {
                    failures.push(format!("{name}: {report}"));
                }
            }
            (Ok(_), Ok(_)) => {}
        }
    }
    failures
}

/// One document's rule passes into a program over parser events, its
/// output rendered as JSON; the output, or the failure.
fn run_passes(program: &Program, make: fn() -> Tabnas, text: &str) -> Result<String, Fail> {
    let (out, outcome) = run_passes_under(program, make, text, &Limits::default());
    outcome.map(|()| out)
}

/// One document's rule passes into a program over parser events under
/// `limits`, its output rendered as JSON: what it wrote, and how it ended.
fn run_passes_under(
    program: &Program,
    make: fn() -> Tabnas,
    text: &str,
    limits: &Limits,
) -> (String, Result<(), Fail>) {
    let buffer = Shared::default();
    let outcome = program
        .parser_sink(
            Box::new(buffer.clone()),
            Some(Renderer::Json),
            limits,
            Metrics::new(),
        )
        .and_then(|sink| {
            let (outcome, _) =
                tabnas_transduce::ParserEventSource::new(make(), text).run_boxed(sink);
            outcome.map(|_| ())
        });
    let bytes = buffer.0.lock().unwrap().clone();
    (String::from_utf8(bytes).expect("utf-8 output"), outcome)
}

/// The library's `transduce-tree`: over every JSON fixture of transduce's,
/// the generated documents and the documents below, for the json part's
/// spec and for specs that announce a named key, take a close pass's
/// value, hand a container on through a replaced rule, name a member
/// inside a rule's own container and yield a rule's text. The json part
/// writes the walk of the grammar's value, and a document the grammar
/// refuses fails as the grammar refuses it, for every spec.
#[test]
fn the_librarys_transduce_tree_writes_each_documents_tree() {
    let specs = [
        (
            "the json part's spec",
            "record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar 0))",
        ),
        (
            "a close pass's value, keyed by name",
            "record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar :value))",
        ),
        (
            "a container handed on through a replaced rule",
            "record\n    entry :list (record (entry :container :array))\n    entry :elem (record (entry :container :array))\n    entry :val (record (entry :scalar 0))",
        ),
        (
            "a key whose token is not a string",
            "record\n    entry :val (record (entry :key 0) (entry :scalar 0))",
        ),
        (
            "a named key on every value",
            "record\n    entry :list (record (entry :container :array))\n    entry :val (record (entry :key \"v\") (entry :scalar 0))",
        ),
        (
            "a pair's value as its text",
            "record\n    entry :map (record (entry :container :object))\n    entry :pair (record (entry :key 0) (entry :text true))",
        ),
        (
            "a container's own member and its text",
            "record\n    entry :list (record (entry :container :object) (entry :member \"items\") (entry :text true))\n    entry :pair (record (entry :member 0))",
        ),
    ];
    let mut documents: Vec<(String, String)> = Vec::new();
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "transduce's fixtures are vendored, at {}: {e}",
                dir.display()
            )
        })
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    entries.sort();
    for path in entries {
        let text = std::fs::read_to_string(&path).expect("a fixture reads");
        documents.push((path.display().to_string(), text));
    }
    for n in [0, 1, 2, 50] {
        documents.push((format!("records_json({n})"), support::records_json(n)));
    }
    for text in [
        "1",
        "2.50",
        "-0",
        "1E+2",
        "\"s\"",
        "null",
        "[]",
        "{}",
        "[[[]],[{}]]",
        r#"{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}"#,
        r#"{"é":"日本","a\"b":"\u0000"}"#,
        r#"{"a":1,"a":2}"#,
        "[1,",
        "{\"a\" 1}",
    ] {
        documents.push((format!("{text:?}"), text.to_string()));
    }
    let mut compared = 0;
    let mut failures = Vec::new();
    for (spec_name, spec) in specs {
        let src = format!("def spec\n  {spec}\n\ndef export [input] (transduce-tree spec input)\n");
        let program = common::compile_with(&src, "tree.alc", tabnas_alchemy::Input::ParserEvents)
            .expect("the program compiles");
        failures.extend(part_over(
            spec_name,
            &program,
            tabnas_json::make,
            &documents,
            spec_name == "the json part's spec",
        ));
        compared += documents.len();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(compared > 100, "{compared} documents compared");
    eprintln!("transduce-tree: {compared} documents written as their trees");
}

/// A `:text` rule's text under a small `max_scalar_bytes`: the library's
/// `string-join` holds it at each of its joins (a pass's tokens together,
/// the innermost text with them, a closed text with the text around it),
/// so at each bound from one that refuses every text to one that refuses
/// none the part writes the document (one its spec is sound for) or
/// refuses it naming `max_scalar_bytes`, some of the refusals the texts'
/// own, not the guard's; the widest bound refuses none.
#[test]
fn a_text_rules_text_is_held_to_max_scalar_bytes() {
    // Each spec over documents it is sound for: a pair's text is a scalar
    // after the pair's key, so no value may be an object (whose own
    // container would stand where the text goes), and a list's member and
    // text are announced inside the list's own object, so no pair, and no
    // list, may lie outside a flat array.
    let specs = [
        (
            "a pair's value as its text",
            "record\n    entry :map (record (entry :container :object))\n    entry :pair (record (entry :key 0) (entry :text true))",
            &[
                r#"{"a":[10,20]}"#,
                r#"{"a":"xyz","bb":[1,[22,333],"x"]}"#,
                r#"{"a":1e21,"b":true,"c":null,"d":[]}"#,
                r#"{"é":"日本"}"#,
                r#"[1,[22,333],"x",{"é":"日本"}]"#,
                r#"{"a":[1,2.50,"x"],"c":[],"e":1e21,"f":true}"#,
            ][..],
        ),
        (
            "a container's own member and its text",
            "record\n    entry :list (record (entry :container :object) (entry :member \"items\") (entry :text true))\n    entry :pair (record (entry :member 0))",
            &[
                "[1,2]",
                "[]",
                r#"[1,"x",true,null,2.50]"#,
                r#"["日本","é"]"#,
                "[1,2,3,4,5,6,7,8,9,10,11,12]",
                r#"[-0,1E+2,"a,b"]"#,
            ][..],
        ),
    ];
    let mut failures = Vec::new();
    let (mut compared, mut texts_refused) = (0, 0);
    for (spec_name, spec, documents) in specs {
        let src = format!("def spec\n  {spec}\n\ndef export [input] (transduce-tree spec input)\n");
        let program = common::compile_with(&src, "tree.alc", tabnas_alchemy::Input::ParserEvents)
            .expect("the program compiles");
        for text in documents {
            for max in [1, 3, 4, 5, 7, 8, 11, 16, 64] {
                let limits = Limits {
                    max_scalar_bytes: max,
                    ..Limits::default()
                };
                let (out, outcome) = run_passes_under(&program, tabnas_json::make, text, &limits);
                match outcome {
                    Ok(()) => assert!(!out.is_empty(), "{spec_name}, {text}, {max}: wrote nothing"),
                    Err(fail)
                        if fail.code == Code::ResourceLimitExceeded
                            && fail.limit.as_ref().map(|l| l.name) == Some("max_scalar_bytes") =>
                    {
                        assert_ne!(
                            max, 64,
                            "{spec_name}, {text}: refused at the widest bound: {fail}"
                        );
                        if fail.message.starts_with("string-join:") {
                            texts_refused += 1;
                        }
                    }
                    Err(fail) => failures.push(format!(
                        "{spec_name}, {text}, max_scalar_bytes {max}: {fail}"
                    )),
                }
                compared += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        texts_refused > 10,
        "{texts_refused} texts refused at the bound"
    );
    eprintln!("transduce-tree: {compared} bounded runs, {texts_refused} texts refused");
}

/// tabnas-csv's own `test/spec` corpus, transduce's CSV fixtures, the
/// generated records and the documents below: what the records tests
/// read.
fn csv_documents() -> Vec<(String, String)> {
    let mut documents: Vec<(String, String)> = Vec::new();
    let corpus = common::vendored().join("csv/test/spec");
    let specs = tabnas_support::load_spec_dir(&corpus, &tabnas_support::SpecOptions::default())
        .unwrap_or_else(|e| panic!("tabnas-csv's corpus is vendored: {e}"));
    for spec in &specs {
        for row in &spec.rows {
            documents.push((
                format!("{}:{}", row.file, row.line),
                row.unesc_named("input"),
            ));
        }
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("transduce's fixtures are vendored")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    entries.sort();
    for path in entries {
        let text = std::fs::read_to_string(&path).expect("a fixture reads");
        documents.push((path.display().to_string(), text));
    }
    for n in [0, 1, 2, 50] {
        documents.push((format!("records_csv({n})"), support::records_csv(n)));
    }
    for text in [
        "",
        "a,b\n",
        "a,b,c\n1,,3\n,5,\n",
        "a,b,c\n1,2\n4\n",
        "a,b\n1,2,3,4\n",
        "a,b\n\"x,y\",\"line\nbreak\"\n\"q\"\"q\",z\n",
        "a,b\r\n1,2\r\n",
        "a,a\n1,2\n",
        " a , b \n 1 , 2 \n",
        "a,b,\n1,2,\n",
        "a,\"b\n",
    ] {
        documents.push((format!("{text:?}"), text.to_string()));
    }
    documents
}

/// The library's `transduce-records`: over tabnas-csv's corpus, transduce's
/// CSV fixtures, the generated records and documents that hold empty cells,
/// short and long records, a header that repeats a name and documents the
/// grammar refuses, for csv's part's spec and for specs without a header,
/// without an empty group and with rules no pass names. csv's part writes
/// the walk of the grammar's value (a header that names a column twice
/// streams both cells, where the value keeps the last: no oracle there),
/// and a document the grammar refuses fails as the grammar refuses it, for
/// every spec.
#[test]
fn the_librarys_transduce_records_writes_each_documents_records() {
    let specs = [
        (
            "csv's part",
            "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :header true\n    entry :unnamed \"field~\"",
        ),
        (
            "no header, null for an empty cell",
            "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :fill null",
        ),
        (
            "a header and no empty group",
            "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :header true",
        ),
        (
            "rules no pass names",
            "record\n    entry :document \"doc\"\n    entry :record \"rec\"\n    entry :cell \"cell\"\n    entry :header true",
        ),
    ];
    let documents = csv_documents();
    let mut compared = 0;
    let mut failures = Vec::new();
    for (spec_name, spec) in specs {
        let src =
            format!("def spec\n  {spec}\n\ndef export [input] (transduce-records spec input)\n");
        let program =
            common::compile_with(&src, "records.alc", tabnas_alchemy::Input::ParserEvents)
                .expect("the program compiles");
        failures.extend(part_over(
            spec_name,
            &program,
            tabnas_csv::make,
            &documents,
            spec_name == "csv's part",
        ));
        compared += documents.len();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(compared > 200, "{compared} documents compared");
    eprintln!("transduce-records: {compared} documents written as their records");
}

/// The state under a small `max_metadata_bytes`: the library's scans hold
/// the header's names and a record's cells by the transduce measure, so at
/// each bound from one that refuses the state before any pass to one that
/// refuses none the part writes the document, the same bytes at every
/// bound that lets it through, or refuses it naming `max_metadata_bytes`.
#[test]
fn transduce_records_state_is_held_to_max_metadata_bytes() {
    let src = "def spec\n  record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :header true\n    entry :unnamed \"field~\"\n\ndef export [input] (transduce-records spec input)\n";
    let program = common::compile_with(src, "records.alc", tabnas_alchemy::Input::ParserEvents)
        .expect("the program compiles");
    let text = "identifier,full name,balance\n1,Ann,2.50\n2,,\n3,Bob,7,extra\n";
    let mut refused = 0;
    let mut written: Option<String> = None;
    for max in (8..=220).step_by(4) {
        let limits = Limits {
            max_metadata_bytes: max,
            ..Limits::default()
        };
        let (out, outcome) = run_passes_under(&program, tabnas_csv::make, text, &limits);
        match outcome {
            Ok(()) => {
                let whole = written.get_or_insert_with(|| out.clone());
                assert_eq!(*whole, out, "max_metadata_bytes {max}");
            }
            Err(fail) => {
                assert_eq!(
                    fail.code,
                    Code::ResourceLimitExceeded,
                    "max_metadata_bytes {max}: {fail}"
                );
                assert_eq!(
                    fail.limit.as_ref().map(|l| l.name),
                    Some("max_metadata_bytes"),
                    "max_metadata_bytes {max}: {fail}"
                );
                refused += 1;
            }
        }
    }
    assert!(refused > 10, "{refused} bounds refused");
    assert!(written.is_some(), "no bound let the document through");
}
