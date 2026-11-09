// Copyright (c) 2026 tabnas, MIT License

package main

// cli_test.go: the dmog binary, built and run the way a script runs it
// (rs/tests/cli_test.rs): the five commands, standard input as -, the
// statuses, and that nothing but the answer reaches standard output.
//
// The binary is built once, with the build tags this test binary has
// (tabnas_nodecell or none). run reads its document through JSON's
// transducer part, which needs no adapter of transduce's, so every case
// runs in either build, and TestRunReadsADocumentInEveryBuild holds that.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	alchemy "github.com/tabnas/alchemy/go"
)

var binary string

func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "dmog-")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	binary = filepath.Join(dir, "dmog")
	args := []string{"build", "-o", binary}
	build := exec.Command("go", append(args, ".")...)
	build.Stdout, build.Stderr = os.Stderr, os.Stderr
	if err := build.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "the dmog binary does not build:", err)
		os.Exit(1)
	}
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}

// output is what one run of the binary left.
type output struct {
	status int
	stdout string
	stderr string
}

func runIn(t *testing.T, dir string, args []string, stdin *string) output {
	t.Helper()
	cmd := exec.Command(binary, args...)
	cmd.Dir = dir
	if stdin != nil {
		// A command refused before standard input is read may have exited
		// already, closing the pipe: that is the behaviour under test, and
		// os/exec does not count the closed pipe as a failure.
		cmd.Stdin = strings.NewReader(*stdin)
	}
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	err := cmd.Run()
	status := 0
	var exit *exec.ExitError
	switch {
	case errors.As(err, &exit):
		status = exit.ExitCode()
	case err != nil:
		t.Fatalf("the binary does not run: %v", err)
	}
	return output{status: status, stdout: stdout.String(), stderr: stderr.String()}
}

func invoke(t *testing.T, args []string, stdin *string) output {
	t.Helper()
	return runIn(t, "", args, stdin)
}

func text(s string) *string { return &s }

// failJSON is the one JSON object on standard error.
func failJSON(t *testing.T, o output) map[string]any {
	t.Helper()
	var f map[string]any
	if err := json.Unmarshal([]byte(strings.TrimSpace(o.stderr)), &f); err != nil {
		t.Fatalf("one JSON object on stderr, not %q", o.stderr)
	}
	return f
}

func tempFile(t *testing.T, name, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

// records is the spec's worked example: aless's
// tests/fixtures/records.json.
const records = `{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]},"payload":{"deep":{"records":[{"id":123,"person":{"name":"Alice"},"account":{"balance":50.25}},{"account":{"balance":72},"person":{"name":"Bob"},"id":456}]}}}}`

const expectedCSV = "\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n"

// program is the spec's program (sections 12.1 and 13.4).
const program = "def column-from-meta [source]\n  record\n    entry :label (get \"title\" source)\n    entry :source\n      as-path\n        get \"path\" source\n\ndef api-binding\n  record\n    entry :columns\n      path \"response\" \"metadata\" \"fields\"\n    entry :rows\n      path \"response\" \"payload\" \"deep\" \"records\" each-index\n    entry :column column-from-meta\n\ndef api-table [input]\n  table-from-json api-binding input\n\ndef export [input]\n  pipe input\n    api-table\n    csv csv-options\n"

const export = "def export [input]\n  pipe input\n    table-from-json api-binding\n    csv csv-options\n"

func TestCanonPrintsTheCanonicalForm(t *testing.T) {
	o := invoke(t, []string{"canon", tempFile(t, "canon.alc", export)}, nil)
	if o.status != 0 || o.stdout != "(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))\n" || o.stderr != "" {
		t.Errorf("%+v", o)
	}
}

func TestFormatPrintsTheLayoutFormAndReadsStandardInput(t *testing.T) {
	o := invoke(t, []string{"format", "-"}, text("(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))"))
	if o.status != 0 || o.stdout != export {
		t.Errorf("%+v", o)
	}
}

func TestCheckIsSilentOnAProgramThatChecks(t *testing.T) {
	o := invoke(t, []string{"check", "-"}, text(program))
	if o.status != 0 || o.stdout != "" || o.stderr != "" {
		t.Errorf("%+v", o)
	}
}

// A library definition over a selection's items, which are typed Value:
// check passes it (the items may be records, or vectors), and run checks
// each one and prints the result.
func TestADefinitionOverSelectedItemsChecksAndRuns(t *testing.T) {
	for _, c := range []struct{ name, program, input, want string }{
		{"cols.alc", "def cols [input] (map public-column (select (path \"cols\" each-index) input))\ndef export [input] (join \",\" (map (fn [c] (get :label c)) (cols input)))\n",
			`{"cols":[{"label":"a","source":1},{"label":"b"}]}`, "a,b"},
		{"rows.alc", "def export [input]\n  concat-map (partial csv-row csv-options) (select (path \"rows\" each-index) input)\n",
			`{"rows":[[1,"x"],[2,"y"]]}`, "\"1\",\"x\"\r\n\"2\",\"y\"\r\n"},
	} {
		path := tempFile(t, c.name, c.program)
		if o := invoke(t, []string{"check", path}, nil); o.status != 0 {
			t.Errorf("%s: %+v", c.name, o)
		}
		if o := invoke(t, []string{"run", path, "-"}, text(c.input)); o.status != 0 || o.stdout != c.want {
			t.Errorf("%s: %+v", c.name, o)
		}
	}
}

func TestCheckReportsAReaderFailureAsJSONOnStderrWithStatus2(t *testing.T) {
	o := invoke(t, []string{"check", "-"}, text("pipe x\n  f\n  []\n"))
	if o.status != 2 || o.stdout != "" {
		t.Errorf("%+v", o)
	}
	f := failJSON(t, o)
	if f["code"] != "DSL_PARSE_ERROR" || !strings.HasPrefix(f["message"].(string), "empty_step: ") || f["row"] != 3.0 || f["col"] != 3.0 || f["output"] != "none" {
		t.Errorf("%v", f)
	}
}

// check reaches the resolver and the checker: an unknown name, a reused
// stream and a recursive definition each fail with their code, at the
// position they name, with status 2.
func TestCheckReportsTheResolverAndCheckerCodes(t *testing.T) {
	// The composed program of the README names a binding it does not
	// define.
	o := invoke(t, []string{"check", "-"}, text(export))
	f := failJSON(t, o)
	if o.status != 2 || f["code"] != "DSL_TYPE_ERROR" || !strings.HasPrefix(f["message"].(string), "unknown_name: api-binding") || f["row"] != 3.0 || f["col"] != 21.0 {
		t.Errorf("%d %v", o.status, f)
	}
	o = invoke(t, []string{"check", "-"}, text("def export [input] (concat (json input) (json input))"))
	f = failJSON(t, o)
	if o.status != 2 || f["code"] != "STREAM_REUSED" || !strings.HasPrefix(f["message"].(string), "reused: ") {
		t.Errorf("%d %v", o.status, f)
	}
	o = invoke(t, []string{"check", "-"}, text("def a [x] (b x)\ndef b [x] (a x)\ndef export [input] (a input)"))
	if o.status != 2 || failJSON(t, o)["code"] != "STREAMABILITY_UNKNOWN" {
		t.Errorf("%+v", o)
	}
}

func TestExplainPrintsThePlanReport(t *testing.T) {
	o := invoke(t, []string{"explain", "-"}, text(program))
	if o.status != 0 || o.stderr != "" ||
		!strings.HasPrefix(o.stdout, "export: api-table → csv\n\nSource reads:          1\n") ||
		!strings.Contains(o.stdout, "Protocol:              JsonEvents/1 → Stream<Selected> → Stream<Value> → TableRows/1 → Text\n") ||
		!strings.Contains(o.stdout, "Row capture:           one .response.payload.deep.records[*], capped at max_record_bytes\n") ||
		!strings.HasSuffix(o.stdout, "A later error can occur after earlier output has been written.\n") {
		t.Errorf("%+v", o)
	}
	o = invoke(t, []string{"explain", "-"}, text("def x 1"))
	if o.status != 2 || !strings.HasPrefix(failJSON(t, o)["message"].(string), "no_export: ") {
		t.Errorf("%+v", o)
	}
}

func TestRunPrintsTheWorkedExampleCsv(t *testing.T) {
	programPath := tempFile(t, "export.alc", program)
	input := tempFile(t, "records.json", records)
	for _, c := range []struct {
		args  []string
		stdin *string
	}{
		{[]string{"run", programPath, input}, nil},
		{[]string{"run", programPath, "-"}, text(records)},
		{[]string{"run", "-", input}, text(program)},
	} {
		if o := invoke(t, c.args, c.stdin); o.status != 0 || o.stdout != expectedCSV || o.stderr != "" {
			t.Errorf("%v: %+v", c.args, o)
		}
	}
}

func TestRunRendersATableResultAsCsvOrJSON(t *testing.T) {
	table := tempFile(t, "table.alc", strings.Replace(program, "    csv csv-options\n", "", 1))
	if o := invoke(t, []string{"run", table, "-"}, text(records)); o.status != 0 || o.stdout != expectedCSV {
		t.Errorf("%+v", o)
	}
	if o := invoke(t, []string{"run", "--render", "json", table, "-"}, text(records)); o.status != 0 ||
		o.stdout != "[{\"Identifier\":123,\"Full name\":\"Alice\",\"Balance\":50.25},{\"Identifier\":456,\"Full name\":\"Bob\",\"Balance\":72}]\n" {
		t.Errorf("%+v", o)
	}
	// The echo.
	echo := tempFile(t, "echo.alc", "def export [input] input\n")
	if o := invoke(t, []string{"run", echo, "-"}, text(`[1.50, {"a": null}]`)); o.status != 0 || o.stdout != "[1.50,{\"a\":null}]\n" {
		t.Errorf("%+v", o)
	}
}

// A renderer refused for a program, before any document is read: CSV for
// a result of JSON events, any renderer for a program that renders its
// own text.
func TestRunRefusesARendererBeforeReading(t *testing.T) {
	echo := tempFile(t, "echo.alc", "def export [input] input\n")
	o := invoke(t, []string{"run", "--render", "csv", echo, "-"}, text("[1]"))
	if o.status != 2 || !strings.HasPrefix(failJSON(t, o)["message"].(string), "protocol_mismatch: ") {
		t.Errorf("%+v", o)
	}
	own := tempFile(t, "text.alc", program)
	o = invoke(t, []string{"run", "--render", "json", own, "-"}, text(records))
	if o.status != 2 || !strings.HasPrefix(failJSON(t, o)["message"].(string), "render_of_text: ") {
		t.Errorf("%+v", o)
	}
	// Before the input is read: an input that cannot be read is never
	// reached, so the refusal is the renderer's, not `cannot read`.
	for _, c := range []struct{ file, render, prefix string }{
		{echo, "csv", "protocol_mismatch: "},
		{own, "json", "render_of_text: "},
	} {
		o = invoke(t, []string{"run", "--render", c.render, c.file, "/nonexistent/input.json"}, nil)
		if o.status != 2 || !strings.HasPrefix(failJSON(t, o)["message"].(string), c.prefix) {
			t.Errorf("%s: %+v", c.prefix, o)
		}
	}
}

// The statuses follow the code: 1 for an input or protocol failure, 5 for
// a limit, 2 for the program; the failure is one JSON object on standard
// error and standard output carries nothing but the answer.
func TestRunStatusesFollowTheFailureCode(t *testing.T) {
	// A program that does not check: status 2, nothing read from the input.
	bad := tempFile(t, "bad.alc", "def export [input] (nope input)\n")
	if o := invoke(t, []string{"run", bad, "-"}, text("not json")); o.status != 2 || failJSON(t, o)["code"] != "DSL_TYPE_ERROR" {
		t.Errorf("%+v", o)
	}
	programPath := tempFile(t, "export2.alc", program)
	// Metadata after the rows: INPUT_ORDER_VIOLATION, status 1, no output.
	reordered := `{"response":{"payload":{"deep":{"records":[{"id":1,"person":{"name":"a"},"account":{"balance":2}}]}},"metadata":{"fields":[{"title":"Identifier","path":["id"]}]}}}`
	o := invoke(t, []string{"run", programPath, "-"}, text(reordered))
	f := failJSON(t, o)
	// The library's text refuses it, so the failure names the library's
	// position rather than the row's path.
	if o.status != 1 || o.stdout != "" || f["code"] != "INPUT_ORDER_VIOLATION" || f["output"] != "none" || !strings.Contains(fmt.Sprint(f["message"]), "rows must follow their metadata") {
		t.Errorf("%d %q %v", o.status, o.stdout, f)
	}
	// Invalid JSON after the rows: INPUT_INVALID, status 1; the rows were
	// buffered, not committed, so nothing reached standard output.
	o = invoke(t, []string{"run", programPath, "-"}, text(records+" x"))
	f = failJSON(t, o)
	if o.status != 1 || o.stdout != "" || f["code"] != "INPUT_INVALID" || f["output"] != "none" {
		t.Errorf("%d %q %v", o.status, o.stdout, f)
	}
	// A missing cell under the standard options: MISSING_VALUE, status 1.
	missing := `{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]}]},"payload":{"deep":{"records":[{"id":1}]}}}}`
	if o := invoke(t, []string{"run", programPath, "-"}, text(missing)); o.status != 1 || failJSON(t, o)["code"] != "MISSING_VALUE" {
		t.Errorf("%+v", o)
	}
}

func TestAReaderErrorCarriesTheEngineCodeAndPosition(t *testing.T) {
	o := invoke(t, []string{"canon", "-"}, text("a\n  b\n c\n"))
	f := failJSON(t, o)
	if o.status != 2 || f["code"] != "DSL_PARSE_ERROR" || !strings.HasPrefix(f["message"].(string), "bad_dedent: ") || f["row"] != 3.0 || f["col"] != 2.0 {
		t.Errorf("%d %v", o.status, f)
	}
}

// The reader bounds nesting, so a program nested far past the bound is a
// too_deep failure with status 2 from every command.
func TestAProgramNestedBeyondTheBoundIsAFailureNotAnAbort(t *testing.T) {
	depth := 100_000
	parens := strings.Repeat("(", depth) + "x" + strings.Repeat(")", depth)
	var indented strings.Builder
	for level := 0; level < 600; level++ {
		indented.WriteString(strings.Repeat("  ", level) + "x\n")
	}
	// Flat to the reader, one level per step to the desugarer.
	pipe := "pipe x" + strings.Repeat(" f", depth) + "\n"
	// The failure names the opener, the line or the form that passed the
	// bound: the 256th paren, the line that would open the 256th level,
	// the pipe.
	for _, c := range []struct {
		command, program string
		row              float64
	}{
		{"canon", parens, 1}, {"format", parens, 1}, {"check", parens, 1}, {"explain", parens, 1},
		{"canon", indented.String(), 257}, {"check", pipe, 1},
	} {
		o := invoke(t, []string{c.command, "-"}, text(c.program))
		if o.status != 2 || o.stdout != "" {
			t.Errorf("%s: %d %q", c.command, o.status, o.stdout)
			continue
		}
		f := failJSON(t, o)
		if f["code"] != "DSL_PARSE_ERROR" || !strings.HasPrefix(f["message"].(string), "too_deep: ") || f["row"] != c.row {
			t.Errorf("%s: %v", c.command, f)
		}
	}
}

func TestUsageErrorsAndUnreadableFilesExit2(t *testing.T) {
	o := invoke(t, nil, nil)
	if o.status != 2 || !strings.Contains(o.stderr, "usage:") || o.stdout != "" {
		t.Errorf("%+v", o)
	}
	// An unknown command is refused before standard input is read, so what
	// it holds does not matter: unparsable input is not a parse error here.
	o = invoke(t, []string{"bogus", "-"}, text("("))
	if o.status != 2 || failJSON(t, o)["code"] != "INPUT_INVALID" || o.stdout != "" {
		t.Errorf("%+v", o)
	}
	// A wrong argument count, a bad renderer, two standard inputs.
	for _, args := range [][]string{
		{"run", "-"},
		{"run", "--render", "xml", "a.alc", "b.json"},
		{"run", "--bogus", "a.alc", "b.json"},
		{"run", "-", "-"},
		{"explain"},
	} {
		o := invoke(t, args, nil)
		if o.status != 2 || failJSON(t, o)["code"] != "INPUT_INVALID" || o.stdout != "" {
			t.Errorf("%v: %+v", args, o)
		}
	}
	o = invoke(t, []string{"canon", "/nonexistent/program.alc"}, nil)
	if o.status != 2 || failJSON(t, o)["code"] != "INPUT_INVALID" || o.stdout != "" {
		t.Errorf("%+v", o)
	}
	// Malformed UTF-8 cannot be read, from a file or from standard input,
	// as in the Rust binary and the TypeScript bin.
	bad := tempFile(t, "bad.alc", "def export [input] \xff\n")
	for _, c := range []struct {
		file  string
		stdin *string
	}{{bad, nil}, {"-", text("\xff")}} {
		o = invoke(t, []string{"canon", c.file}, c.stdin)
		f := failJSON(t, o)
		if o.status != 2 || f["code"] != "INPUT_INVALID" || f["message"] != "cannot read "+c.file+": stream did not contain valid UTF-8" || o.stdout != "" {
			t.Errorf("%s: %+v", c.file, o)
		}
	}
}

// A program inside the nesting bound checks, explains and runs: a pipe of
// 245 steps.
func TestAProgramInsideTheBoundRuns(t *testing.T) {
	path := tempFile(t, "pipe245.alc", "def export [input]\n  pipe input\n    select (path each-index)\n"+strings.Repeat("    map (fn [x] x)\n", 245)+"    join \",\"\n")
	for _, command := range []string{"check", "explain"} {
		if o := invoke(t, []string{command, path}, nil); o.status != 0 {
			t.Errorf("%s: %+v", command, o)
		}
	}
	if o := invoke(t, []string{"run", path, "-"}, text(`["a","b"]`)); o.status != 0 || o.stdout != "a,b" {
		t.Errorf("%+v", o)
	}
}

// A function applied to itself is recursion with status 2 from check
// (which builds the plan) and from run.
func TestSelfApplicationIsAFailureNotAnAbort(t *testing.T) {
	omega := tempFile(t, "omega.alc", "def w [f] (f f)\ndef export [input]\n  let [x (w w)]\n    json input\n")
	o := invoke(t, []string{"check", omega}, nil)
	f := failJSON(t, o)
	if o.status != 2 || f["code"] != "STREAMABILITY_UNKNOWN" || !strings.HasPrefix(f["message"].(string), "recursion: ") {
		t.Errorf("%d %v", o.status, f)
	}
	perItem := tempFile(t, "omega-item.alc", "def w [f] (f f)\ndef export [input]\n  pipe input\n    select (path each-index)\n    map (fn [x] (w w))\n    join \",\"\n")
	o = invoke(t, []string{"run", perItem, "-"}, text("[1]"))
	f = failJSON(t, o)
	if o.status != 2 || f["code"] != "STREAMABILITY_UNKNOWN" || !strings.HasPrefix(f["message"].(string), "recursion: ") {
		t.Errorf("%d %v", o.status, f)
	}
}

// check builds the plan, so a fail on export's own path is reported
// there, with its code (INPUT_INVALID, status 1) and the form's position,
// although no document was read.
func TestCheckReportsAFailThePlanReaches(t *testing.T) {
	o := invoke(t, []string{"check", tempFile(t, "fail.alc", "def export [input] (concat \"a\" (fail \"boom\"))\n")}, nil)
	f := failJSON(t, o)
	if o.status != 1 || f["code"] != "INPUT_INVALID" || f["message"] != "boom" || f["row"] != 1.0 || f["col"] != 32.0 {
		t.Errorf("%d %v", o.status, f)
	}
}

// A failure inside the standard library names the library's file, row and
// column in its message, and gives no row or column of the user's file; a
// program that happens to share a library file's name is still read as
// its own text.
func TestALibraryFailureNamesTheLibraryFile(t *testing.T) {
	lib, _ := alchemy.StdlibSource("stdlib/table.alc")
	row, col := 0, 0
	for i, line := range strings.Split(lib, "\n") {
		if strings.Contains(line, `fail "Required metadata was not found"`) {
			row, col = i+1, strings.Index(line, "fail")+1
		}
	}
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "stdlib"), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"export.alc", "stdlib/table.alc"} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(program), 0o644); err != nil {
			t.Fatal(err)
		}
		o := runIn(t, dir, []string{"run", name, "-"}, text("{}"))
		f := failJSON(t, o)
		if o.status != 1 || f["code"] != "INPUT_INVALID" || f["message"] != fmt.Sprintf("Required metadata was not found (at stdlib/table.alc:%d:%d)", row, col) {
			t.Errorf("%s: %d %v", name, o.status, f)
		}
		if _, has := f["row"]; has {
			t.Errorf("%s: %v", name, f)
		}
	}
}

// A standard error that cannot be written loses the report, not the
// status: the failure's own status comes back, never a signal's.
func TestAClosedStandardErrorKeepsTheStatus(t *testing.T) {
	bad := tempFile(t, "closed.alc", "def export [input] (nope input)\n")
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	r.Close()
	cmd := exec.Command(binary, "check", bad)
	cmd.Stderr = w
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	w.Close()
	err = cmd.Wait()
	var exit *exec.ExitError
	if !errors.As(err, &exit) || exit.ExitCode() != 2 {
		t.Errorf("%v", err)
	}
}

// --max-output-bytes bounds what run writes, and a text longer than it
// fails with the limit named, status 5.
func TestRunTakesAnOutputLimit(t *testing.T) {
	programPath := tempFile(t, "limit.alc", program)
	if o := invoke(t, []string{"run", "--max-output-bytes", "many", programPath, "-"}, text(records)); o.status != 2 || failJSON(t, o)["code"] != "INPUT_INVALID" {
		t.Errorf("%+v", o)
	}
	o := invoke(t, []string{"run", "--max-output-bytes", "20", programPath, "-"}, text(records))
	f := failJSON(t, o)
	if limit, _ := f["limit"].(map[string]any); o.status != 5 || limit["name"] != "max_output_bytes" || len(o.stdout) > 20 {
		t.Errorf("%d %v %q", o.status, f, o.stdout)
	}
	if o := invoke(t, []string{"run", "--max-output-bytes", "1000", programPath, "-"}, text(records)); o.status != 0 || o.stdout != expectedCSV {
		t.Errorf("%+v", o)
	}
}

// run reads its document through JSON's transducer part in every build,
// with the tabnas_nodecell tag or without it: a number keeps its lexeme,
// and a repeated member streams as the document spells it, twice.
func TestRunReadsADocumentInEveryBuild(t *testing.T) {
	echo := tempFile(t, "echo.alc", "def export [input] (json input)\n")
	doc := `{"a":[1.50,1e2],"a":2}`
	if o := invoke(t, []string{"run", echo, "-"}, text(doc)); o.status != 0 || o.stdout != doc+"\n" || o.stderr != "" {
		t.Errorf("%+v", o)
	}
}

// transducer is the json transducer: the library's transduce-tree over the
// json grammar's rules, as its module's part spells it.
const transducer = "def export [input]\n  transduce-tree\n    record\n      entry :map (record (entry :container :object))\n      entry :list (record (entry :container :array))\n      entry :pair (record (entry :key 0))\n      entry :val (record (entry :scalar 0))\n    input\n"

// run --grammar FORMAT parses INPUT with that format's grammar and runs the
// program over its rule passes, in every build (the source of rule passes
// needs no adapter): a transducer over json's writes the document, a
// number's lexeme kept; a document the grammar
// refuses is INPUT_INVALID; a format that is not parsed, or is unknown, is
// refused with status 2, naming the formats that are (C is parsed here,
// where the Rust crate reads it as its realized tree; pgn and feed are
// read as trees here); a program over JSON events, compiled over parser
// events, is a protocol mismatch; and the option takes a value.
func TestRunTakesAGrammarWhoseRulePassesTheProgramReads(t *testing.T) {
	programPath := tempFile(t, "transducer.alc", transducer)
	doc := `{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}`
	if o := invoke(t, []string{"run", "--grammar", "json", programPath, "-"}, text(doc)); o.status != 0 || o.stdout != doc+"\n" {
		t.Errorf("%+v", o)
	}
	if o := invoke(t, []string{"run", "--grammar", "json", programPath, "-"}, text("[1,")); o.status != 1 || o.stdout != "" || failJSON(t, o)["code"] != "INPUT_INVALID" {
		t.Errorf("%+v", o)
	}
	for _, name := range []string{"abnf", "nope"} {
		o := invoke(t, []string{"run", "--grammar", name, programPath, "-"}, text(doc))
		f := failJSON(t, o)
		want := "--grammar takes a format whose documents are parsed (c, css, csv, feed, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, semver, toml, xml, yaml or zon), not " + fmt.Sprintf("%q", name)
		if o.status != 2 || f["code"] != "INPUT_INVALID" || f["message"] != want {
			t.Errorf("%s: %+v", name, o)
		}
	}
	events := tempFile(t, "events.alc", "def export [input] (json input)\n")
	o := invoke(t, []string{"run", "--grammar", "json", events, "-"}, text(doc))
	f := failJSON(t, o)
	if message, _ := f["message"].(string); o.status == 0 || f["code"] != "DSL_TYPE_ERROR" || !strings.HasPrefix(message, "protocol_mismatch") {
		t.Errorf("%+v", o)
	}
	if o := invoke(t, []string{"run", programPath, "-", "--grammar"}, text(doc)); o.status != 2 {
		t.Errorf("%+v", o)
	}
}
