// The `dmog` binary, run the way a script runs it: the five program
// commands, and what `formats` and `translate` answer of the formats'
// declared refusals; standard input as `-`, the statuses, and that nothing
// but the answer reaches standard output.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dmog"))
}

fn run(args: &[&str], stdin: Option<&str>) -> Output {
    run_bytes(args, stdin.map(str::as_bytes))
}

/// `run` with standard input as bytes, which need not be UTF-8. They are
/// written on a thread of their own while the output is read: `run` reads
/// its input through a reader and writes as it goes, so a long input
/// written whole first would wait on an output pipe nobody was reading.
fn run_bytes(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(binary())
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the dmog binary starts");
    let writer = stdin.map(|text| {
        let mut pipe = child.stdin.take().expect("a piped stdin");
        let text = text.to_vec();
        std::thread::spawn(move || {
            // A command refused before standard input is read, or one that
            // fails part way through it, may have exited already, closing
            // the pipe: that is the behaviour under test, not a failure to
            // write.
            if let Err(error) = pipe.write_all(&text) {
                assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
            }
        })
    });
    let output = child.wait_with_output().expect("the binary finishes");
    if let Some(writer) = writer {
        writer.join().expect("standard input is written");
    }
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 output")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 output")
}

fn fail_json(output: &Output) -> serde_json::Value {
    serde_json::from_str(stderr(output).trim())
        .unwrap_or_else(|_| panic!("one JSON object on stderr, not {:?}", stderr(output)))
}

fn temp_file(name: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dmog-{}-{name}", std::process::id()));
    std::fs::write(&path, text).expect("the file is written");
    path
}

/// The spec's worked example: aless's `tests/fixtures/records.json`.
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

const EXPORT: &str =
    "def export [input]\n  pipe input\n    table-from-json api-binding\n    csv csv-options\n";

#[test]
fn canon_prints_the_canonical_form() {
    let file = temp_file("canon.alc", EXPORT);
    let output = run(&["canon", file.to_str().expect("a utf-8 path")], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))\n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn format_prints_the_layout_form_and_reads_standard_input() {
    let output = run(
        &["format", "-"],
        Some("(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))"),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), EXPORT);
}

#[test]
fn check_is_silent_on_a_program_that_checks() {
    let output = run(&["check", "-"], Some(PROGRAM));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");
}

/// A library definition over a selection's items, which are typed
/// `Value`: `check` passes it (the items may be records, or vectors), and
/// `run` checks each one and prints the result.
#[test]
fn a_definition_over_selected_items_checks_and_runs() {
    let cases = [
        (
            "cols.alc",
            "def cols [input] (map public-column (select (path \"cols\" each-index) input))\ndef export [input] (join \",\" (map (fn [c] (get :label c)) (cols input)))\n",
            r#"{"cols":[{"label":"a","source":1},{"label":"b"}]}"#,
            "a,b",
        ),
        (
            "rows.alc",
            "def export [input]\n  concat-map (partial csv-row csv-options) (select (path \"rows\" each-index) input)\n",
            r#"{"rows":[[1,"x"],[2,"y"]]}"#,
            "\"1\",\"x\"\r\n\"2\",\"y\"\r\n",
        ),
    ];
    for (name, program, input, want) in cases {
        let path = temp_file(name, program);
        let path = path.to_str().unwrap();
        let out = run(&["check", path], None);
        assert_eq!(out.status.code(), Some(0), "{name}: {}", stderr(&out));
        let out = run(&["run", path, "-"], Some(input));
        assert_eq!(out.status.code(), Some(0), "{name}: {}", stderr(&out));
        assert_eq!(stdout(&out), want, "{name}");
    }
}

#[test]
fn check_reports_a_reader_failure_as_json_on_stderr_with_status_2() {
    let output = run(&["check", "-"], Some("pipe x\n  f\n  []\n"));
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "DSL_PARSE_ERROR");
    assert!(
        fail["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("empty_step: ")),
        "{fail}"
    );
    assert_eq!(fail["row"], 3);
    assert_eq!(fail["col"], 3);
    assert_eq!(fail["output"], "none");
}

/// `check` now reaches the resolver and the checker: an unknown name, a
/// reused stream and a dynamic function each fail with their code, at
/// the position they name, with status 2.
#[test]
fn check_reports_the_resolver_and_checker_codes() {
    // The composed program of the README names a binding it does not
    // define.
    let output = run(&["check", "-"], Some(EXPORT));
    assert_eq!(output.status.code(), Some(2));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "DSL_TYPE_ERROR");
    assert!(
        fail["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("unknown_name: api-binding")),
        "{fail}"
    );
    assert_eq!(
        (fail["row"].as_u64(), fail["col"].as_u64()),
        (Some(3), Some(21))
    );

    let output = run(
        &["check", "-"],
        Some("def export [input] (concat (json input) (json input))"),
    );
    assert_eq!(output.status.code(), Some(2));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "STREAM_REUSED");
    assert!(
        fail["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("reused: ")),
        "{fail}"
    );

    let output = run(
        &["check", "-"],
        Some("def a [x] (b x)\ndef b [x] (a x)\ndef export [input] (a input)"),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fail_json(&output)["code"], "STREAMABILITY_UNKNOWN");
}

#[test]
fn explain_prints_the_plan_report() {
    let output = run(&["explain", "-"], Some(PROGRAM));
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("export: api-table → csv\n\nSource reads:          1\n"),
        "{text}"
    );
    assert!(
        text.contains("Protocol:              JsonEvents/1 → Stream<Selected> → Stream<Value> → TableRows/1 → Text\n"),
        "{text}"
    );
    assert!(text.contains("Row capture:           one .response.payload.deep.records[*], capped at max_record_bytes\n"), "{text}");
    assert!(
        text.ends_with("A later error can occur after earlier output has been written.\n"),
        "{text}"
    );
    assert_eq!(stderr(&output), "");

    let output = run(&["explain", "-"], Some("def x 1"));
    assert_eq!(output.status.code(), Some(2));
    assert!(
        fail_json(&output)["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("no_export: ")),
        "{}",
        stderr(&output)
    );
}

#[test]
fn run_prints_the_worked_example_csv() {
    let program = temp_file("export.alc", PROGRAM);
    let program = program.to_str().expect("a utf-8 path");
    let input = temp_file("records.json", RECORDS);
    let input = input.to_str().expect("a utf-8 path");
    for args in [
        vec!["run", program, input],
        vec!["run", program, "-"],
        vec!["run", "-", input],
    ] {
        let stdin = match args.as_slice() {
            [.., "-"] => Some(RECORDS),
            [_, "-", _] => Some(PROGRAM),
            _ => None,
        };
        let output = run(&args, stdin);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        assert_eq!(stdout(&output), EXPECTED_CSV, "{args:?}");
        assert_eq!(stderr(&output), "", "{args:?}");
    }
}

/// The example programs under `examples/` run as the README says: the
/// worked example renders its table as CSV, and the table alone leaves the
/// rendering to the command.
#[test]
fn the_examples_run_as_the_readme_says() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let examples = examples.to_str().expect("a utf-8 path");
    let records = format!("{examples}/records.json");
    assert_eq!(
        std::fs::read_to_string(&records).unwrap().trim_end(),
        RECORDS,
        "the example document is the worked example's"
    );
    let output = run(&["run", &format!("{examples}/export.alc"), &records], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), EXPECTED_CSV);
    let table = format!("{examples}/table.alc");
    let output = run(&["run", &table, &records], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), EXPECTED_CSV);
    let output = run(&["run", "--render", "json", &table, &records], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "[{\"Identifier\":123,\"Full name\":\"Alice\",\"Balance\":50.25},{\"Identifier\":456,\"Full name\":\"Bob\",\"Balance\":72}]\n"
    );
}

#[test]
fn run_renders_a_table_result_as_csv_or_json() {
    let table = PROGRAM.replace("    csv csv-options\n", "");
    let program = temp_file("table.alc", &table);
    let program = program.to_str().expect("a utf-8 path");
    let output = run(&["run", program, "-"], Some(RECORDS));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), EXPECTED_CSV);
    let output = run(&["run", "--render", "json", program, "-"], Some(RECORDS));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "[{\"Identifier\":123,\"Full name\":\"Alice\",\"Balance\":50.25},{\"Identifier\":456,\"Full name\":\"Bob\",\"Balance\":72}]\n"
    );
    // The echo, and a renderer refused for a program that renders itself.
    let echo = temp_file("echo.alc", "def export [input] input\n");
    let echo = echo.to_str().expect("a utf-8 path");
    let output = run(&["run", echo, "-"], Some("[1.50, {\"a\": null}]"));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "[1.50,{\"a\":null}]\n");
    let output = run(&["run", "--render", "csv", echo, "-"], Some("[1]"));
    assert_eq!(output.status.code(), Some(2));
    assert!(
        fail_json(&output)["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("protocol_mismatch: ")),
        "{}",
        stderr(&output)
    );
    let output = run(&["run", "--render", "json", program, "-"], Some(RECORDS));
    assert!(output.status.success());
    let text = temp_file("text.alc", PROGRAM);
    let output = run(
        &[
            "run",
            "--render",
            "json",
            text.to_str().expect("a utf-8 path"),
            "-",
        ],
        Some(RECORDS),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        fail_json(&output)["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("render_of_text: ")),
        "{}",
        stderr(&output)
    );
    // Before the input is read: an input that cannot be read is never
    // reached, so the refusal is the renderer's, not `cannot read`.
    let text = text.to_str().expect("a utf-8 path");
    for (file, render, prefix) in [
        (echo, "csv", "protocol_mismatch: "),
        (text, "json", "render_of_text: "),
    ] {
        let output = run(
            &["run", "--render", render, file, "/nonexistent/input.json"],
            None,
        );
        assert_eq!(output.status.code(), Some(2), "{prefix}");
        assert!(
            fail_json(&output)["message"]
                .as_str()
                .is_some_and(|m| m.starts_with(prefix)),
            "{}",
            stderr(&output)
        );
    }
}

/// The statuses follow the code: 1 for an input or protocol failure, 5
/// for a limit, 2 for the program; the failure is one JSON object on
/// standard error and standard output carries nothing but the answer.
#[test]
fn run_statuses_follow_the_failure_code() {
    let program = temp_file("export2.alc", PROGRAM);
    let program = program.to_str().expect("a utf-8 path");
    // Metadata after the rows: INPUT_ORDER_VIOLATION, status 1, no output.
    let reordered = r#"{"response":{"payload":{"deep":{"records":[{"id":1,"person":{"name":"a"},"account":{"balance":2}}]}},"metadata":{"fields":[{"title":"Identifier","path":["id"]}]}}}"#;
    let output = run(&["run", program, "-"], Some(reordered));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "INPUT_ORDER_VIOLATION");
    assert_eq!(fail["output"], "none");
    // The library's text refuses it, so the failure names the library's
    // position rather than the row's path.
    assert!(
        fail["message"]
            .as_str()
            .is_some_and(|m| m.contains("rows must follow their metadata")),
        "{fail}"
    );
    // Invalid JSON after the rows: INPUT_INVALID, status 1; the rows were
    // buffered, not committed, so nothing reached standard output.
    let output = run(&["run", program, "-"], Some(&format!("{RECORDS} x")));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "INPUT_INVALID");
    assert_eq!(fail["output"], "none");
    // A missing cell under the standard options: MISSING_VALUE, status 1.
    let missing = r#"{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]}]},"payload":{"deep":{"records":[{"id":1}]}}}}"#;
    let output = run(&["run", program, "-"], Some(missing));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(fail_json(&output)["code"], "MISSING_VALUE");
    // A program that does not check: status 2, nothing read from the input.
    let bad = temp_file("bad.alc", "def export [input] (nope input)\n");
    let output = run(
        &["run", bad.to_str().expect("a utf-8 path"), "-"],
        Some("not json"),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fail_json(&output)["code"], "DSL_TYPE_ERROR");
}

#[test]
fn a_reader_error_carries_the_engine_code_and_position() {
    let output = run(&["canon", "-"], Some("a\n  b\n c\n"));
    assert_eq!(output.status.code(), Some(2));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "DSL_PARSE_ERROR");
    assert!(
        fail["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("bad_dedent: ")),
        "{fail}"
    );
    assert_eq!(
        (fail["row"].as_u64(), fail["col"].as_u64()),
        (Some(3), Some(2))
    );
}

/// The reader bounds nesting, so a program nested far past the bound is a
/// `too_deep` failure with status 2 from every command, where it used to
/// take the process down with a stack overflow (status -6, nothing on
/// standard error but the runtime's abort).
#[test]
fn a_program_nested_beyond_the_bound_is_a_failure_not_an_abort() {
    let depth = 100_000;
    let parens = format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
    let mut indented = String::new();
    for level in 0..600 {
        indented.push_str(&"  ".repeat(level));
        indented.push_str("x\n");
    }
    // Flat to the reader, one level per step to the desugarer.
    let pipe = format!("pipe x{}\n", " f".repeat(depth));
    // The failure names the opener, the line or the form that passed the
    // bound: the 256th paren, the line that would open the 256th level,
    // the pipe.
    for (command, program, row) in [
        ("canon", &parens, 1),
        ("format", &parens, 1),
        ("check", &parens, 1),
        ("explain", &parens, 1),
        ("canon", &indented, 257),
        ("check", &pipe, 1),
    ] {
        let output = run(&[command, "-"], Some(program));
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), "", "{command}");
        let fail = fail_json(&output);
        assert_eq!(fail["code"], "DSL_PARSE_ERROR", "{command}");
        assert!(
            fail["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("too_deep: ")),
            "{command}: {fail}"
        );
        assert_eq!(fail["row"], row, "{command}: {fail}");
    }
}

#[test]
fn usage_errors_and_unreadable_files_exit_2() {
    let output = run(&[], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage:"), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");

    // An unknown command is refused before standard input is read, so
    // what it holds does not matter: unparsable input is not a parse
    // error here.
    let output = run(&["bogus", "-"], Some("("));
    assert_eq!(output.status.code(), Some(2));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "INPUT_INVALID");
    assert_eq!(stdout(&output), "");

    // A wrong argument count, a bad renderer, two standard inputs.
    for args in [
        vec!["run", "-"],
        vec!["run", "--render", "xml", "a.alc", "b.json"],
        vec!["run", "--bogus", "a.alc", "b.json"],
        vec!["run", "-", "-"],
        vec!["explain"],
    ] {
        let output = run(&args, None);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(fail_json(&output)["code"], "INPUT_INVALID", "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
    }

    let output = run(&["canon", "/nonexistent/program.alc"], None);
    assert_eq!(output.status.code(), Some(2));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "INPUT_INVALID");
    assert_eq!(stdout(&output), "");

    // Malformed UTF-8 cannot be read, from a file or from standard input,
    // as in the TypeScript bin and the Go command.
    let bad = std::env::temp_dir().join(format!("dmog-{}-bad.alc", std::process::id()));
    std::fs::write(&bad, b"def export [input] \xff\n").expect("the file is written");
    let bad = bad.to_str().expect("a utf-8 path");
    for (file, stdin) in [(bad, None), ("-", Some(&b"\xff"[..]))] {
        let output = run_bytes(&["canon", file], stdin);
        assert_eq!(output.status.code(), Some(2), "{file}");
        let fail = fail_json(&output);
        assert_eq!(fail["code"], "INPUT_INVALID", "{file}");
        assert_eq!(
            fail["message"],
            format!("cannot read {file}: stream did not contain valid UTF-8")
        );
        assert_eq!(stdout(&output), "", "{file}");
    }
}

/// A program inside the nesting bound checks, runs and explains in a
/// debug build too: the command runs on a thread of the stack the checker
/// and the evaluator are promised, where the main thread's smaller one
/// overflowed on a pipe of 245 steps.
#[test]
fn a_program_inside_the_bound_runs_in_a_debug_build() {
    let program = format!(
        "def export [input]\n  pipe input\n    select (path each-index)\n{}    join \",\"\n",
        "    map (fn [x] x)\n".repeat(245)
    );
    let program = temp_file("pipe245.alc", &program);
    let program = program.to_str().expect("a utf-8 path");
    for args in [vec!["check", program], vec!["explain", program]] {
        let output = run(&args, None);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    let output = run(&["run", program, "-"], Some(r#"["a","b"]"#));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "a,b");
}

/// A function applied to itself is `recursion` with status 2 from `check`
/// (which builds the plan) and from `run`, not a stack overflow that
/// aborts the process.
#[test]
fn self_application_is_a_failure_not_an_abort() {
    let omega = temp_file(
        "omega.alc",
        "def w [f] (f f)\ndef export [input]\n  let [x (w w)]\n    json input\n",
    );
    let output = run(&["check", omega.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "STREAMABILITY_UNKNOWN");
    assert!(
        fail["message"].as_str().unwrap().starts_with("recursion: "),
        "{fail}"
    );
    let per_item = temp_file(
        "omega-item.alc",
        "def w [f] (f f)\ndef export [input]\n  pipe input\n    select (path each-index)\n    map (fn [x] (w w))\n    join \",\"\n",
    );
    let output = run(&["run", per_item.to_str().unwrap(), "-"], Some("[1]"));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(fail_json(&output)["code"], "STREAMABILITY_UNKNOWN");
}

/// `check` builds the plan, so a `fail` on `export`'s own path is reported
/// there, with its code (`INPUT_INVALID`, status 1) and the form's
/// position, although no document was read.
#[test]
fn check_reports_a_fail_the_plan_reaches() {
    let program = temp_file(
        "fail.alc",
        "def export [input] (concat \"a\" (fail \"boom\"))\n",
    );
    let output = run(&["check", program.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let fail = fail_json(&output);
    assert_eq!(fail["code"], "INPUT_INVALID");
    assert_eq!(fail["message"], "boom");
    assert_eq!(
        (fail["row"].as_u64(), fail["col"].as_u64()),
        (Some(1), Some(32))
    );
}

/// A failure inside the standard library names the library's file, row
/// and column in its message, and gives no row or column of the user's
/// file; a program that happens to share a library file's name is still
/// read as its own text.
#[test]
fn a_library_failure_names_the_library_file() {
    let lib = tabnas_alchemy::stdlib::source("stdlib/table.alc").unwrap();
    let (row, line) = lib
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains("fail \"Required metadata was not found\""))
        .unwrap();
    let (row, col) = (row + 1, line.find("fail").unwrap() + 1);
    let dir = std::env::temp_dir().join(format!("dmog-{}-lib", std::process::id()));
    std::fs::create_dir_all(dir.join("stdlib")).unwrap();
    for name in ["export.alc", "stdlib/table.alc"] {
        let path = dir.join(name);
        std::fs::write(&path, PROGRAM).unwrap();
        let output = Command::new(binary())
            .current_dir(&dir)
            .args(["run", name, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(b"{}")?;
                child.wait_with_output()
            })
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{name}: {}", stderr(&output));
        let fail = fail_json(&output);
        assert_eq!(fail["code"], "INPUT_INVALID", "{name}");
        assert_eq!(
            fail["message"],
            format!("Required metadata was not found (at stdlib/table.alc:{row}:{col})"),
            "{name}"
        );
        assert!(fail.get("row").is_none(), "{name}: {fail}");
    }
}

/// A standard error that cannot be written loses the report, not the
/// status: the failure's own status comes back, never the 101 of a panic.
#[test]
fn a_closed_standard_error_keeps_the_status() {
    let bad = temp_file("closed.alc", "def export [input] (nope input)\n");
    let mut child = Command::new(binary())
        .args(["check", bad.to_str().unwrap()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    assert_eq!(child.wait().unwrap().code(), Some(2));
}

/// `--max-output-bytes` bounds what `run` writes, and a text longer than
/// it fails with the limit named, status 5.
#[test]
fn run_takes_an_output_limit() {
    let program = temp_file("limit.alc", PROGRAM);
    let program = program.to_str().unwrap();
    let output = run(
        &["run", "--max-output-bytes", "20", program, "-"],
        Some(RECORDS),
    );
    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    let fail = fail_json(&output);
    assert_eq!(fail["limit"]["name"], "max_output_bytes");
    assert!(output.stdout.len() <= 20);
    let output = run(
        &["run", "--max-output-bytes", "1000", program, "-"],
        Some(RECORDS),
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), EXPECTED_CSV);
    let output = run(
        &["run", "--max-output-bytes", "many", program, "-"],
        Some(RECORDS),
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fail_json(&output)["code"], "INPUT_INVALID");
}

/// `formats` lists every format `translate` reads and writes, by id; those
/// that write a schema's tree with no embedding into it are schema-only.
#[test]
fn formats_lists_the_formats_translate_reads_and_writes() {
    let out = run(&["formats"], None);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let formats: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    let formats = formats.as_array().expect("an array");
    let ids: Vec<&str> = formats.iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids.join(" "),
        "abnf c css csv ebnf expr feed gbnf ini json json5 jsonc jsonic jsonl markdown pgn proto \
         semver toml xml yaml zon"
    );
    let schema_only: Vec<&str> = formats
        .iter()
        .filter(|f| !f["schema"].is_null() && f["writes"] == "tree" && f["embed"].is_null())
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        schema_only,
        ["abnf", "c", "css", "ebnf", "gbnf", "pgn", "proto"]
    );
    // The grammar notations share a schema, a grammar's rules in source
    // order, so each writes the others' documents.
    let schema =
        |id: &str| formats.iter().find(|f| f["id"] == id).expect("a format")["schema"].clone();
    for id in ["abnf", "ebnf", "gbnf"] {
        assert_eq!(schema(id), "grammar-rules", "{id}");
    }
    assert_eq!(schema("c"), "c");
    // Why a format's documents are read whole, where its manifest says:
    // every format transduce does not stream, each in the sentence its
    // manifest gives; JSON's none, nor PGN's, which streams a game at a
    // time, nor CSS's, which streams a statement at a time, nor INI's and
    // TOML's, which stream a member at a time, nor feed's, which streams
    // an entry at a time, nor proto's, which streams a declaration at a
    // time, nor Markdown's, which streams a block at a time, nor the
    // grammar notations', which stream a rule at a time, nor C's, which
    // streams a translation unit's items as each ends, each through the
    // part it ships.
    let by_id = |id: &str| formats.iter().find(|f| f["id"] == id).expect("a format");
    for (id, manifest) in [
        ("expr", tabnas_expr::translate().unwrap().manifest),
        ("semver", tabnas_semver::translate().unwrap().manifest),
    ] {
        let manifest: serde_json::Value = serde_json::from_str(manifest).expect("JSON");
        let whole = &manifest["translate"]["whole"];
        assert!(whole.as_str().is_some_and(|w| !w.is_empty()), "{id}");
        assert_eq!(&by_id(id)["whole"], whole, "{id}");
    }
    assert!(by_id("json")["whole"].is_null());
    assert!(by_id("pgn")["whole"].is_null());
    assert!(by_id("css")["whole"].is_null());
    assert!(by_id("ini")["whole"].is_null());
    assert!(by_id("toml")["whole"].is_null());
    assert!(by_id("feed")["whole"].is_null());
    assert!(by_id("proto")["whole"].is_null());
    assert_eq!(by_id("proto")["transduce"], "proto-transduce");
    assert_eq!(by_id("proto")["schema"], "proto-ast");
    assert!(by_id("markdown")["whole"].is_null());
    assert_eq!(by_id("markdown")["transduce"], "markdown-transduce");
    for id in ["abnf", "c", "ebnf", "gbnf"] {
        assert!(by_id(id)["whole"].is_null(), "{id}");
        assert_eq!(by_id(id)["transduce"], format!("{id}-transduce"), "{id}");
    }
    let keys: Vec<&str> = by_id("css")
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "id",
            "reads",
            "writes",
            "root",
            "schema",
            "whole",
            "lift",
            "embed",
            "render",
            "transduce",
            "loss"
        ]
    );
}

/// What a target declares it cannot carry it refuses before writing
/// anything, status 1: a schema-only target refuses another format's tree
/// when the route is composed, before the input is read, a grammar
/// notation's render refuses a grammar it has no form for, naming the
/// rule and what it met, and Semantic Versioning's embedding refuses a
/// tree that is not a version. Each takes its own format's documents, and
/// a program that makes its tree; a grammar notation takes the others'
/// too. The TypeScript and Go commands write the same.
#[test]
fn translate_refuses_what_a_target_cannot_carry() {
    let out = run(
        &[
            "translate",
            "--from",
            "json",
            "--to",
            "css",
            "/nonexistent/input.json",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        fail_json(&out),
        serde_json::json!({
            "code": "TARGET_VALUE_UNREPRESENTABLE",
            "message": "schema_only: css writes a css-ast tree, the tree its own documents read as, \
                        and this document is not one; a program that makes one can be composed with \
                        the render",
            "output": "none"
        })
    );
    let out = run(
        &["translate", "--from", "json", "--to", "semver", "-"],
        Some(r#"{"major":1,"minor":2}"#),
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        fail_json(&out),
        serde_json::json!({
            "code": "TARGET_VALUE_UNREPRESENTABLE",
            "message": "the document is not a version: it has no patch; a version is an object whose \
                        major, minor and patch are whole numbers written in digits, with an optional \
                        prerelease and build, each a list of identifiers or one string of them",
            "row": 48,
            "col": 3,
            "file": "tabnas-semver/alchemy/embed.alc",
            "output": "none"
        })
    );
    for (to, schema) in [("abnf", "grammar-rules"), ("c", "c")] {
        let out = run(
            &["translate", "--from", "json", "--to", to, "-"],
            Some("{}"),
        );
        assert_eq!(out.status.code(), Some(1), "{to}: {}", stderr(&out));
        assert_eq!(stdout(&out), "", "{to}");
        assert_eq!(
            fail_json(&out),
            serde_json::json!({
                "code": "TARGET_VALUE_UNREPRESENTABLE",
                "message": format!(
                    "schema_only: {to} writes a {schema} tree, the tree its own documents read as, \
                     and this document is not one; a program that makes one can be composed with \
                     the render"
                ),
                "output": "none"
            }),
            "{to}"
        );
    }
    let out = run(
        &["translate", "--from", "abnf", "--to", "ebnf", "-"],
        Some("greet = \"hi\"\n"),
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        fail_json(&out),
        serde_json::json!({
            "code": "TARGET_VALUE_UNREPRESENTABLE",
            "message": "the grammar cannot be written as EBNF: the rule \"greet\" holds the \
                        case-insensitive literal \"hi\", which holds a letter beside another \
                        character, and no one EBNF terminal spells",
            "row": 83,
            "col": 3,
            "file": "tabnas-ebnf/alchemy/render.alc",
            "output": "none"
        })
    );
    let echo = temp_file("echo-css.alc", "def export [input] input\n");
    let echo = echo.to_str().unwrap();
    let tree = r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#;
    for (args, input, want) in [
        (
            &["--from", "json", "--to", "semver", "-"][..],
            r#"{"major":1,"minor":2,"patch":3,"prerelease":"rc.1"}"#,
            "1.2.3-rc.1",
        ),
        (
            &["--from", "semver", "--to", "json", "-"][..],
            "1.2.3-rc.1",
            "{\"major\":1,\"minor\":2,\"patch\":3,\"prerelease\":[\"rc\",1],\"build\":[]}\n",
        ),
        (
            &["--from", "css", "--to", "css", "-"][..],
            "a{color:red}",
            "a {\n  color: red;\n}\n",
        ),
        (
            &["--from", "json", "--to", "css", "--with", echo, "-"][..],
            tree,
            "a {\n  color: red;\n}\n",
        ),
        (
            &["--from", "pgn", "--to", "pgn", "-"][..],
            "1. e4 e5 1-0",
            "1. e4 e5 1-0\n",
        ),
        (
            &["--from", "json", "--to", "expr", "-"][..],
            r#"{"a":[1,-2]}"#,
            "{\"a\":[1,-2]}\n",
        ),
        (
            &["--from", "expr", "--to", "json", "-"][..],
            "1+2*3\n",
            "[\"+\",1,[\"*\",2,3]]\n",
        ),
        // White space after the last token is not in the tree (C's loss
        // list), so the final line feed is not written.
        (
            &["--from", "c", "--to", "c", "-"][..],
            "int main(void) { return 0; }\n",
            "int main(void) { return 0; }",
        ),
        (
            &["--from", "ebnf", "--to", "abnf", "-"][..],
            "top ::= \"a\" b\nb ::= \"c\"\n",
            "top = %s\"a\" b\nb = %s\"c\"\n",
        ),
        (
            &["--from", "ebnf", "--to", "gbnf", "-"][..],
            "top ::= \"a\" b\nb ::= \"c\"\n",
            "top ::= \"a\" b\nb ::= \"c\"\nroot ::= top\n",
        ),
        (
            &["--from", "abnf", "--to", "ebnf", "-"][..],
            "top = \"a\"\n",
            "top ::= [aA]\n",
        ),
    ] {
        let out = run(&[&["translate"][..], args].concat(), Some(input));
        assert_eq!(out.status.code(), Some(0), "{args:?}: {}", stderr(&out));
        assert_eq!(stdout(&out), want, "{args:?}");
        assert_eq!(stderr(&out), "", "{args:?}");
    }
}

/// What a package's own parse checks before its parser runs, the command
/// checks too, and refuses with status 1, writing nothing: a .proto file
/// nesting past proto's cap is refused at the brace that opens the level
/// past it, as proto's parser reads it, with the code `proto_too_deep`,
/// before the engine builds a tree that deep, whose drop could abort the
/// process, and a JSON5 document holding no value with
/// json5's own codes (`tabnas_json5::check_reader`, which `parse_reader_with`
/// makes), not the engine's `unexpected`, from a file and from standard
/// input. A grammar notation's document is read as its rules, which no
/// compiler has seen, so one its compiler would refuse is read as it is.
/// The TypeScript and Go commands write the same, but that TypeScript's
/// json5 error carries no position.
#[test]
fn translate_checks_what_a_packages_parse_checks_first() {
    let deep = format!(
        "syntax = \"proto3\";\n{}{}\n",
        "message M {".repeat(101),
        "}".repeat(101)
    );
    let out = run(
        &["translate", "--from", "proto", "--to", "json", "-"],
        Some(&deep),
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert_eq!(
        fail_json(&out),
        serde_json::json!({
            "code": "INPUT_INVALID",
            "message": "proto_too_deep: proto: document nests deeper than the 100 levels this parser accepts",
            "row": 2,
            "col": 1111,
            "output": "none"
        })
    );
    for (input, code) in [("", "json5_empty"), ("// c\n", "json5_no_value")] {
        let file = temp_file(&format!("{code}.json5"), input);
        let file = file.to_str().expect("a utf-8 path");
        for (path, stdin) in [(file, None), ("-", Some(input))] {
            let out = run(
                &["translate", "--from", "json5", "--to", "json", path],
                stdin,
            );
            assert_eq!(out.status.code(), Some(1), "{path}: {}", stderr(&out));
            assert_eq!(stdout(&out), "", "{path}");
            assert_eq!(
                fail_json(&out),
                serde_json::json!({
                    "code": "INPUT_INVALID",
                    "message": format!("{code}: JSON5 input must contain a value"),
                    "row": 1,
                    "col": 1,
                    "output": "none"
                }),
                "{path}"
            );
        }
    }
    // A document that opens with a comment and holds a value is read.
    let out = run(
        &["translate", "--from", "json5", "--to", "json", "-"],
        Some("// c\n{a:1}\n"),
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "{\"a\":1}\n");
    // A grammar notation's document is read as its rules in source order:
    // a rule referencing one the grammar does not define is read as it is,
    // and so is a left recursion through a cycle of twenty rules, each with
    // two recursive alternates, which tabnas-bnf's elimination refuses
    // past 512 alternatives.
    let out = run(
        &["translate", "--from", "abnf", "--to", "json", "-"],
        Some("a = \"b\" c\n"),
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "{\"type\":\"grammar\",\"rules\":[{\"type\":\"rule\",\"name\":\"a\",\"alternatives\":\
         [[{\"type\":\"literal\",\"text\":\"b\",\"caseSensitive\":false},\
         {\"type\":\"reference\",\"name\":\"c\"}]]}]}\n"
    );
    let k = 20;
    let mut cycle = String::from("root ::= r0\n");
    for i in 0..k {
        let next = (i + 1) % k;
        cycle.push_str(&format!(
            "r{i} ::= r{next} \"a{i}\" | r{next} \"b{i}\" | \"x{i}\"\n"
        ));
    }
    let out = run(
        &["translate", "--from", "gbnf", "--to", "json", "-"],
        Some(&cycle),
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let tree: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(tree["start"], "root");
    assert_eq!(tree["rules"].as_array().map(Vec::len), Some(k + 1));
}

/// INPUT that cannot be read exits 2 as `cannot read`, on every route a
/// command reads it by:
/// - `run`, through a reader, a window at a time for JSON;
/// - `run --grammar`, through a reader for a grammar that declares the
///   window (json, and json5, whose check reads only as far as the
///   document's leading whitespace and comments), and whole first for one
///   that does not (yaml);
/// - `translate`, through a reader on the transducer part's route, and
///   whole on the others (a path, a format whose package ships no part,
///   standard input).
///
/// Bytes that are not UTF-8, a file that is not there and a directory are
/// each refused as reading the file whole refuses them, before any
/// output.
#[test]
fn an_input_that_cannot_be_read_exits_2_on_every_route() {
    let echo = temp_file("unread-echo.alc", "def export [input] input\n");
    let echo = echo.to_str().expect("a utf-8 path");
    let transducer = temp_file("unread-transducer.alc", TRANSDUCER);
    let transducer = transducer.to_str().expect("a utf-8 path");
    let bytes = &b"[1, \"\xff\"]"[..];
    let bad = std::env::temp_dir().join(format!("dmog-{}-bad-input", std::process::id()));
    std::fs::write(&bad, bytes).expect("the file is written");
    let bad = bad.to_str().expect("a utf-8 path");
    let dir = std::env::temp_dir();
    let dir = dir.to_str().expect("a utf-8 path");
    let commands: [&[&str]; 9] = [
        &["run", echo],
        &["run", "--grammar", "json", transducer],
        &["run", "--grammar", "yaml", transducer],
        &["run", "--grammar", "json5", transducer],
        &["translate", "--from", "json", "--to", "json"],
        &["translate", "--from", "yaml", "--to", "json"],
        &["translate", "--from", "json5", "--to", "json"],
        &["translate", "--from", "toml", "--to", "json"],
        &[
            "translate",
            "--from",
            "json",
            "--to",
            "json",
            "--path",
            "[0]",
        ],
    ];
    for command in commands {
        for (input, stdin) in [
            (bad, None),
            ("-", Some(bytes)),
            ("/nonexistent/input.json", None),
            (dir, None),
        ] {
            let reason = match input {
                "-" => "stream did not contain valid UTF-8".to_string(),
                file => std::fs::read_to_string(file).unwrap_err().to_string(),
            };
            let output = run_bytes(&[command, &[input]].concat(), stdin);
            let at = format!("{command:?} {input}");
            assert_eq!(output.status.code(), Some(2), "{at}: {}", stderr(&output));
            let fail = fail_json(&output);
            assert_eq!(fail["code"], "INPUT_INVALID", "{at}");
            assert_eq!(
                fail["message"],
                format!("cannot read {input}: {reason}"),
                "{at}"
            );
            assert_eq!(fail["output"], "none", "{at}");
            assert_eq!(stdout(&output), "", "{at}");
        }
    }
}

/// `run` reads INPUT through a reader, a window of it at a time, so a long
/// document's output streams before the parse reaches its end. Bytes that
/// are not UTF-8 near the end fail the run as a file that cannot be read,
/// after the output of the document before them, and the failure says the
/// output is partial: from a file and from standard input alike.
#[test]
fn run_streams_a_long_input_until_bytes_that_are_not_utf8() {
    let echo = temp_file("stream-echo.alc", "def export [input] input\n");
    let echo = echo.to_str().expect("a utf-8 path");
    let records: Vec<String> = (0..20_000)
        .map(|i| format!("{{\"id\":{i},\"name\":\"n{i}\"}}"))
        .collect();
    let records = records.join(",");
    let good = temp_file("stream-good.json", &format!("[{records}]"));
    let whole = run(&["run", echo, good.to_str().expect("a utf-8 path")], None);
    assert_eq!(whole.status.code(), Some(0), "{}", stderr(&whole));
    let whole = stdout(&whole);
    let bytes = [b"[", records.as_bytes(), b",\"\xff\"]"].concat();
    let bad = std::env::temp_dir().join(format!("dmog-{}-stream-bad.json", std::process::id()));
    std::fs::write(&bad, &bytes).expect("the file is written");
    let bad = bad.to_str().expect("a utf-8 path");
    for (input, stdin) in [(bad, None), ("-", Some(&bytes[..]))] {
        let output = run_bytes(&["run", echo, input], stdin);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{input}: {}",
            stderr(&output)
        );
        let fail = fail_json(&output);
        assert_eq!(
            fail["message"],
            format!("cannot read {input}: stream did not contain valid UTF-8"),
            "{input}"
        );
        assert_eq!(fail["output"], "partial", "{input}");
        let written = stdout(&output);
        assert!(
            written.len() >= 32 * 1024,
            "{input}: {} bytes",
            written.len()
        );
        assert!(whole.starts_with(&written), "{input}");
    }
}

/// The json transducer: the library's `transduce-tree` over the json
/// grammar's rules, as its package's part spells it.
const TRANSDUCER: &str = "def export [input]
  transduce-tree
    record
      entry :map (record (entry :container :object))
      entry :list (record (entry :container :array))
      entry :pair (record (entry :key 0))
      entry :val (record (entry :scalar 0))
    input
";

/// `run --grammar FORMAT` parses INPUT with that format's grammar and runs
/// the program over its rule passes: a transducer over json's writes the
/// document, a number's lexeme kept; a
/// document the grammar refuses is INPUT_INVALID; a format that is not
/// parsed, or is unknown, is refused with status 2, naming the formats
/// that are; a program over JSON events, compiled over parser events, is a
/// protocol mismatch; and the option takes a value.
#[test]
fn run_takes_a_grammar_whose_rule_passes_the_program_reads() {
    let program = temp_file("transducer.alc", TRANSDUCER);
    let program = program.to_str().expect("a utf-8 path");
    let doc = r#"{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}"#;
    let out = run(&["run", "--grammar", "json", program, "-"], Some(doc));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), format!("{doc}\n"));
    let out = run(&["run", "--grammar", "json", program, "-"], Some("[1,"));
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");
    assert_eq!(fail_json(&out)["code"], "INPUT_INVALID");
    for name in ["expr", "nope"] {
        let out = run(&["run", "--grammar", name, program, "-"], Some(doc));
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        let fail = fail_json(&out);
        assert_eq!(fail["code"], "INPUT_INVALID");
        assert_eq!(
            fail["message"],
            format!(
                "--grammar takes a format whose documents are parsed (abnf, c, css, csv, ebnf, \
                 feed, gbnf, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, proto, semver, \
                 toml, xml, yaml or zon), not {name:?}"
            )
        );
    }
    let events = temp_file("events.alc", "def export [input] (json input)\n");
    let out = run(
        &["run", "--grammar", "json", events.to_str().unwrap(), "-"],
        Some(doc),
    );
    assert_ne!(out.status.code(), Some(0));
    let fail = fail_json(&out);
    assert_eq!(fail["code"], "DSL_TYPE_ERROR");
    assert!(fail["message"]
        .as_str()
        .is_some_and(|m| m.starts_with("protocol_mismatch")));
    let out = run(&["run", program, "-", "--grammar"], Some(doc));
    assert_eq!(out.status.code(), Some(2));
}
