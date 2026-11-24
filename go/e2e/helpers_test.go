// Copyright (c) 2026 tabnas, MIT License

package e2e

// helpers_test.go: what the run tests share (alchemy's go/helpers_test.go):
// the stages, the spec's worked example, the documents transduce's tests
// generate in its shape (transduce/rs/tests/support/mod.rs), the ways a
// test reads a document into a program's sink, and a runtime over a
// program's text (alchemy's go/interp_test.go).

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	. "github.com/tabnas/alchemy/go"
	translate "github.com/tabnas/dmog/go/translate"
	tabnasjson "github.com/tabnas/json/go"
	tabnas "github.com/tabnas/parser/go"
	tr "github.com/tabnas/render/go"
	tt "github.com/tabnas/transduce/go"
)

// routers and renderers are the stages the tests run programs on, as a
// host hands them in: transduce's and render's.
var (
	routers   = tt.Routers()
	renderers = tr.Renderers()
)

// records is the spec's worked example: aless's
// tests/fixtures/records.json, byte for byte (329 bytes; the metadata
// before the rows; Bob's members in another order; 50.25 and 72 as
// written).
const records = `{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]},"payload":{"deep":{"records":[{"id":123,"person":{"name":"Alice"},"account":{"balance":50.25}},{"account":{"balance":72},"person":{"name":"Bob"},"id":456}]}}}}`

// expectedCSV is what the worked example prints.
const expectedCSV = "\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n"

// metadata is the metadata the worked example carries: three columns by
// path.
const metadata = `{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]}`

// record is one record, as compact JSON.
func record(i int) string {
	return fmt.Sprintf(`{"id":%d,"person":{"name":"Person number %d"},"account":{"balance":%d.%02d}}`, i, i, i*7, i%100)
}

// recordsJSON is the worked-example document with n records.
func recordsJSON(n int) string {
	var b strings.Builder
	b.WriteString(`{"response":{"metadata":` + metadata + `,"payload":{"deep":{"records":[`)
	for i := 0; i < n; i++ {
		if i > 0 {
			b.WriteByte(',')
		}
		b.WriteString(record(i))
	}
	b.WriteString("]}}}}")
	return b.String()
}

// recordsJSONL is the records alone, one JSON document per line.
func recordsJSONL(n int) string {
	var b strings.Builder
	for i := 0; i < n; i++ {
		b.WriteString(record(i))
		b.WriteByte('\n')
	}
	return b.String()
}

// recordsCSV is the records flattened to id,name,balance, with a header
// line.
func recordsCSV(n int) string {
	var b strings.Builder
	b.WriteString("id,name,balance\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "%d,Person number %d,%d.%02d\n", i, i, i*7, i%100)
	}
	return b.String()
}

// recordsYAML is the worked-example document as block YAML.
func recordsYAML(n int) string {
	var b strings.Builder
	b.WriteString("response:\n  metadata:\n    fields:\n      - title: Identifier\n        path: [id]\n      - title: Full name\n        path: [person, name]\n      - title: Balance\n        path: [account, balance]\n  payload:\n    deep:\n      records:\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "        - id: %d\n          person:\n            name: Person number %d\n          account:\n            balance: %d.%02d\n", i, i, i*7, i%100)
	}
	return b.String()
}

// shaped is the worked-example document around these records.
func shaped(recordsText string) string {
	return `{"response":{"metadata":` + metadata + `,"payload":{"deep":{"records":[` + recordsText + `]}}}}`
}

// transduceFixtures is transduce's fixture directory, vendored, as the Rust
// tests find it (rs/tests/run_test.rs).
func transduceFixtures(t testing.TB) string {
	t.Helper()
	dir := filepath.Join(repoRoot(t), "..", "transduce", "rs", "tests", "fixtures")
	if _, err := os.Stat(dir); err != nil {
		t.Fatalf("transduce's fixtures are vendored, at %s: %v", dir, err)
	}
	return dir
}

// recordEvents is the events of one document, read by parser through the
// materialized walk (sound for every grammar), as the Rust tests record
// them to replay.
func recordEvents(parser *tabnas.Tabnas, text string) ([]tt.Event, *Fail) {
	var rec tt.Recorder
	if _, f := tt.NewParserSource(parser, text).Run(&rec); f != nil {
		return nil, f
	}
	return rec.Events, nil
}

// mustEvents is the json grammar's events of text, or the test's end.
func mustEvents(t testing.TB, text string) []tt.Event {
	t.Helper()
	events, f := recordEvents(tabnasjson.Make(), text)
	if f != nil {
		t.Fatalf("%s: %v", text, f)
	}
	return events
}

// replayEvents replays events through program's sink: the output, or the
// failure.
func replayEvents(program *Program, events []tt.Event, render Renderer, limits tt.Limits, metrics *tt.Metrics) (string, *Fail) {
	var buffer bytes.Buffer
	sink, f := program.Sink(&buffer, render, limits, metrics)
	if f != nil {
		return "", f
	}
	_, f = tt.Replay(events, sink)
	return buffer.String(), f
}

// replayRun reads doc with the json grammar's materialized walk and
// replays its events through program's sink.
func replayRun(t testing.TB, program *Program, doc string, render Renderer, limits tt.Limits, metrics *tt.Metrics) (string, *Fail) {
	t.Helper()
	return replayEvents(program, mustEvents(t, doc), render, limits, metrics)
}

// jsonInto pushes the events of the JSON document doc into sink, as
// `dmog run` reads its input (rs/tests/common/mod.rs's json_into):
// JSON's own transducer part over the parser's rule passes, the engine's
// arrays emptied behind it. It needs no adapter of transduce's, so it reads
// in every build.
func jsonInto(doc string, limits tt.Limits, metrics *tt.Metrics, sink tt.Sink) *Fail {
	json := translate.Named("json")
	if json == nil {
		panic("the command reads JSON")
	}
	part, f := json.TransducerProgram()
	if f != nil {
		return f
	}
	if part == nil {
		panic("the JSON grammar ships a transducer part")
	}
	head, f := part.ParserSinkInto(sink, limits, metrics)
	if f != nil {
		return f
	}
	source, f := json.Passes(doc)
	if f != nil {
		return f
	}
	_, f = source.Grammar(json.ID()).Prune().Limits(limits).Metrics(metrics).Run(head)
	return f
}

// runtimeOf is a runtime over src, named t.alc, on the tests' stages
// (alchemy's go/interp_test.go).
func runtimeOf(t testing.TB, src string) *Runtime {
	t.Helper()
	return runtimeNamed(t, src, "t.alc")
}

func runtimeNamed(t testing.TB, src, file string) *Runtime {
	t.Helper()
	forms, f := ParseFile(src, file)
	if f == nil {
		forms, f = Desugar(forms, src)
	}
	if f != nil {
		t.Fatalf("%q: %v", src, f)
	}
	sources := OneSource(file, src)
	resolved, f := Resolve(forms, sources, Outer)
	if f != nil {
		t.Fatalf("%q: %v", src, f)
	}
	return NewRuntime(resolved, sources).WithRouters(routers).WithRenderers(renderers)
}
