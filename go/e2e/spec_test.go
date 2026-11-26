// Copyright (c) 2026 tabnas, MIT License

package e2e

// spec_test.go: alchemy's shared fixtures, vendored (test/fixtures/alchemy), that run
// programs, through tabnas/support's runner as every grammar repository
// runs its own: run.tsv, a program run over a JSON document, the bytes it
// writes or the failure, on transduce's routers and render's renderers;
// transduce.tsv, a program run over a grammar's rule passes, the bytes it
// writes or the failure, held to the walk of the grammar's value where it
// answers JSON events; and the catalogue drift test over the failures of
// all five fixtures, which needs their rows. From alchemy's go/spec_test.go: reader.tsv,
// pipe.tsv and check.tsv run there, with the drift test's checks of the
// rows they meet.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"testing"

	. "github.com/tabnas/alchemy/go"
	translate "github.com/tabnas/dmog/go/translate"
	tabnasjson "github.com/tabnas/json/go"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
	tt "github.com/tabnas/transduce/go"
)

// specDir is alchemy's fixtures, in its checkout.
func specDir(t testing.TB) string {
	t.Helper()
	return filepath.Join(repoRoot(t), "test", "spec")
}

// repoRoot is alchemy's vendored test files (test/fixtures/alchemy from
// dmog's root; the tests run in go/e2e), and the other repositories' are
// beside it, each under its own name, as tools/vendor-fixtures.sh writes
// them. A missing file fails the test, naming the path.
func repoRoot(t testing.TB) string {
	t.Helper()
	fixtures := os.Getenv("DMOG_FIXTURES") // the sibling checkouts' directory, when the gates run against them
	if fixtures == "" {
		fixtures = filepath.Join("..", "..", "test", "fixtures")
	}
	root, err := filepath.Abs(filepath.Join(fixtures, "alchemy"))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(root, "test", "spec")); err != nil {
		t.Fatalf("alchemy's fixtures are vendored at %s: run tools/vendor-fixtures.sh: %v", root, err)
	}
	return root
}

// failError is a *Fail as the runner sees it: an error whose code is the
// one a row pins (runCode: the finer code for alchemy's own codes,
// the transduce code for any other, in every file, as test/AGENTS.md asks
// of a port's runners) and whose position is the failure's.
type failError struct{ f *Fail }

func (e failError) Error() string { return e.f.Error() }

func asErr(f *Fail) error {
	if f == nil {
		return nil
	}
	return failError{f}
}

func failOf(err error) (*Fail, bool) {
	var fe failError
	if errors.As(err, &fe) {
		return fe.f, true
	}
	return nil, false
}

// readerRow, pipeRow and checkRow are what alchemy's reader.tsv,
// pipe.tsv and check.tsv rows become (alchemy's go/spec_test.go runs
// them); the drift test below meets their failures.
func readerRow(input string) (string, *Fail) {
	program, f := Parse(input)
	if f != nil {
		return "", f
	}
	return Canonical(program), nil
}

func pipeRow(input string) (string, *Fail) {
	program, f := Parse(input)
	if f != nil {
		return "", f
	}
	core, f := Desugar(program, input)
	if f != nil {
		return "", f
	}
	return Canonical(core), nil
}

// checkRow is a program that checks as its plan report; one that does not
// fails with the resolver's, the checker's or the plan evaluation's code,
// at the position it names. input is what the host declares.
func checkRow(src string, input Input) (string, *Fail) {
	program, f := CompileWith(src, "check", input, routers, renderers)
	if f != nil {
		return "", f
	}
	return program.Explain(), nil
}

// runCode is the code a run row pins: the finer code (the first word of
// the message) for alchemy's own codes, which carry one, and the
// transduce or render code itself for every other failure, whose message
// is free text (`fail "a: b"` must not pin `a`).
func runCode(f *Fail) string {
	switch f.Code {
	case CodeDSLParseError, CodeDSLTypeError, CodeStreamReused, CodeStreamabilityUnknown:
		return FinerCode(f)
	}
	return f.Code.String()
}

// runRender is the renderer a run row names.
func runRender(t testing.TB, row *support.Row) Renderer {
	name := row.Named("render")
	if name == "" {
		return RenderDefault
	}
	render, ok := RendererNamed(name)
	if !ok {
		t.Fatalf("%s: render is csv, json or empty, not %q", row.Where(), name)
	}
	return render
}

// runDoc is the document a run row reads: an empty cell is null.
func runDoc(row *support.Row) string {
	if doc := row.UnescNamed("doc"); doc != "" {
		return doc
	}
	return "null"
}

// runOutcome is one run of a row's program: the bytes it wrote, or the
// failure.
type runOutcome struct {
	out  string
	fail *Fail
}

func (o runOutcome) String() string {
	if o.fail != nil {
		return "Err(" + o.fail.Error() + ")"
	}
	return fmt.Sprintf("Ok(%q)", o.out)
}

// runOne compiles program and runs it over doc as `dmog run` does: the
// document read through JSON's transducer part, with the default limits.
func runOne(program, doc string, render Renderer) runOutcome {
	compiled, f := Compile(program, "run", routers, renderers)
	if f != nil {
		return runOutcome{fail: f}
	}
	out, f := driveRun(compiled, doc, render, tt.DefaultLimits(), nil)
	return runOutcome{out: out, fail: f}
}

// driveRun reads doc as `dmog run` reads its input (jsonInto) and pushes
// its events through the program's sink: the output, or the failure.
// metrics may be nil.
func driveRun(program *Program, doc string, render Renderer, limits tt.Limits, metrics *tt.Metrics) (string, *Fail) {
	if metrics == nil {
		metrics = tt.NewMetrics()
	}
	var buffer bytes.Buffer
	sink, f := program.Sink(&buffer, render, limits, metrics)
	if f != nil {
		return "", f
	}
	f = jsonInto(doc, limits, metrics, sink)
	return buffer.String(), f
}

// TestSpecRun runs run.tsv: a program run over a JSON document, the bytes
// it writes or the failure.
func TestSpecRun(t *testing.T) {
	spec, err := support.LoadSpec(filepath.Join(specDir(t), "run.tsv"), nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(spec.Rows) == 0 {
		t.Fatal("run.tsv holds no rows")
	}
	if got := strings.Join(spec.Rows[0].Header, " "); got != "input expected doc render" {
		t.Fatalf("run.tsv's columns are %s", got)
	}
	r := support.Runner{
		ParseRow: func(input string, row *support.Row) (any, error) {
			o := runOne(input, runDoc(row), runRender(t, row))
			if o.fail != nil {
				return nil, asErr(o.fail)
			}
			return o.out, nil
		},
		ErrorCode: func(err error) string {
			if f, ok := failOf(err); ok {
				return runCode(f)
			}
			return err.Error()
		},
		ErrorPos: func(err error) (int, int, bool) {
			if f, ok := failOf(err); ok && f.Row > 0 {
				return int(f.Row), int(f.Column), true
			}
			return 0, 0, false
		},
	}
	for _, row := range spec.Rows {
		input := row.Unesc(0)
		t.Run(fmt.Sprintf("row %d", row.Line), func(t *testing.T) {
			if err := r.CheckRow(row, input, row.Col(1)); err != nil {
				t.Error(err)
			}
		})
	}
}

// rowFailures is what a row's program meets, as the runners above meet it:
// the reader's (reader.tsv), the desugarer's (pipe.tsv), compile's
// (check.tsv, over the input its protocol column declares: empty for JSON
// events, parser for the parser's rule passes), or a run's (run.tsv).
func rowFailures(t *testing.T, file string, row *support.Row) (fails []*Fail) {
	input := row.Unesc(0)
	var f *Fail
	switch file {
	case "reader.tsv":
		_, f = readerRow(input)
	case "pipe.tsv":
		_, f = pipeRow(input)
	case "check.tsv":
		declared := InputJSONEvents
		switch protocol := row.Named("protocol"); protocol {
		case "":
		case "parser":
			declared = InputParserEvents
		default:
			t.Fatalf("%s: the protocol is empty or parser, not %q", row.Where(), protocol)
		}
		_, f = checkRow(input, declared)
	case "transduce.tsv":
		grammar, render, duplicates := transduceGrammar(t, row), runRender(t, row), runDuplicates(t, row)
		if _, o := runTransduce(input, grammar, row.UnescNamed("doc"), render, duplicates); o.fail != nil {
			fails = append(fails, o.fail)
		}
		return fails
	default:
		if o := runOne(input, runDoc(row), runRender(t, row)); o.fail != nil {
			fails = append(fails, o.fail)
		}
		return fails
	}
	if f != nil {
		fails = append(fails, f)
	}
	return fails
}

// placeholder is a template's `{name}`; finalPlaceholder one that ends it,
// with the space before it.
var (
	placeholder      = regexp.MustCompile(`\{[a-z_]+\}`)
	finalPlaceholder = regexp.MustCompile(`\s*\{[a-z_]+\}$`)
)

// fills reports whether text fills the template line: its fixed parts in
// order, each `{name}` any text, none included.
func fills(line, text string) bool {
	parts := placeholder.Split(line, -1)
	if len(parts) == 1 {
		return text == line
	}
	first, last := parts[0], parts[len(parts)-1]
	if len(text) < len(first)+len(last) || !strings.HasPrefix(text, first) || !strings.HasSuffix(text, last) {
		return false
	}
	end := len(text) - len(last)
	at := len(first)
	for _, part := range parts[1 : len(parts)-1] {
		found := strings.Index(text[at:end], part)
		if found < 0 {
			return false
		}
		at += found + len(part)
	}
	return true
}

// instanceOf reports whether text is an instance of the template line. The
// engine trims the messages it writes from a template, so a `{name}` that
// ends a line may take the space before it with it.
func instanceOf(line, text string) bool {
	if fills(line, text) {
		return true
	}
	bare := finalPlaceholder.ReplaceAllString(line, "")
	return bare != line && fills(bare, text)
}

// fixedLength is the fixed text of a template: what a more specific line
// has more of.
func fixedLength(line string) int {
	return len(placeholder.ReplaceAllString(line, ""))
}

// TestTheRaisedMessagesMatchTheDocument: every failure the shared fixtures
// meet is declared in the grammar document (rs/tests/spec_test.rs,
// the_raised_messages_match_the_document). The finer code that leads the
// message is a key of the installed error messages, the engine's and this
// grammar's, and the text after it is a line of that entry, each `{name}`
// standing for what the raising site fills in (a failure raised inside the
// standard library ends with its position there, ` (at stdlib/...)`, which
// is not part of the text). Each line of each code the document declares is
// the most specific line some row meets, so the document holds no text
// nothing raises, and each of its codes has a hint. A finer code comes
// with the same code wherever it is raised (bad_let is a DSL_PARSE_ERROR
// from the desugarer and from the resolver alike). A raising site whose
// code or text drifts from the document fails here.
//
// A build without the incremental adapter cannot run the run rows that
// read their document; it checks the failures it meets, and leaves the
// lines only those rows meet to a build with the adapter.
//
// This is the whole test, over every error row of all four fixtures.
// alchemy runs it too, over reader.tsv's, pipe.tsv's and check.tsv's
// rows alone, without the clause that each line is met, which needs
// run.tsv's.
func TestTheRaisedMessagesMatchTheDocument(t *testing.T) {
	installed := Make().Config().ErrorMessages
	doc, err := documentValue()
	if err != nil {
		t.Fatal(err)
	}
	options := doc["options"].(map[string]any)
	declared := options["error"].(map[string]any)
	hints := options["hint"].(map[string]any)
	specs, err := support.LoadSpecDir(specDir(t), nil)
	if err != nil {
		t.Fatal(err)
	}
	met := map[string]bool{}
	// The codes each finer code is raised with: one, wherever it is raised.
	raisedAs := map[string]map[Code]bool{}
	failures := 0
	for _, spec := range specs {
		for _, row := range spec.Rows {
			if !support.IsErrorExpect(row.Col(1)) {
				continue
			}
			for _, f := range rowFailures(t, spec.Name, row) {
				switch f.Code {
				case CodeDSLParseError, CodeDSLTypeError, CodeStreamReused, CodeStreamabilityUnknown:
				default:
					continue
				}
				failures++
				code, text, ok := strings.Cut(f.Message, ": ")
				if !ok {
					t.Errorf("%s: %s has no finer code: %s", row.Where(), f.Code, f.Message)
					continue
				}
				if raisedAs[code] == nil {
					raisedAs[code] = map[Code]bool{}
				}
				raisedAs[code][f.Code] = true
				if library := strings.LastIndex(text, " (at stdlib/"); library >= 0 && strings.HasSuffix(text, ")") {
					text = text[:library]
				}
				entry, ok := installed[code]
				if !ok {
					t.Errorf("%s: %s is not declared in options.error", row.Where(), code)
					continue
				}
				best := ""
				found := false
				for _, line := range strings.Split(entry, "\n") {
					if instanceOf(line, text) && (!found || fixedLength(line) > fixedLength(best)) {
						best, found = line, true
					}
				}
				if !found {
					t.Errorf("%s: no line of options.error.%s is %q", row.Where(), code, text)
					continue
				}
				met[code+"\n"+best] = true
			}
		}
	}
	if failures == 0 {
		t.Fatal("the fixtures meet no failures")
	}
	for code, uppers := range raisedAs {
		if len(uppers) > 1 {
			names := make([]string, 0, len(uppers))
			for upper := range uppers {
				names = append(names, upper.String())
			}
			sort.Strings(names)
			t.Errorf("%s is raised as %s; a finer code has one code", code, strings.Join(names, " and "))
		}
	}
	for code := range declared {
		if _, ok := hints[code].(string); !ok {
			t.Errorf("options.hint.%s is not declared", code)
		}
	}
	codes := make([]string, 0, len(declared))
	for code := range declared {
		codes = append(codes, code)
	}
	sort.Strings(codes)
	for _, code := range codes {
		entry, _ := declared[code].(string)
		for _, line := range strings.Split(entry, "\n") {
			if !met[code+"\n"+line] {
				t.Errorf("options.error.%s: no fixture row meets %q", code, line)
			}
		}
	}
}

// documentValue is the grammar document as a plain value, as alchemy's own
// (unexported) documentValue reads it: GrammarText, the text alchemy
// embeds, read with the strict JSON grammar, comment lexing on.
func documentValue() (map[string]any, error) {
	reader := tabnasjson.Make()
	on := true
	reader.SetOptions(tabnas.Options{Comment: &tabnas.CommentOptions{Lex: &on}})
	value, err := reader.Parse(GrammarText())
	if err != nil {
		return nil, fmt.Errorf("alchemy-grammar.jsonic does not read: %w", err)
	}
	data, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	var out map[string]any
	err = json.Unmarshal(data, &out)
	return out, err
}

// runDuplicates is the policy a transduce row's duplicates column names
// for a member name an object repeats within a captured value: empty is
// the default, first keeps the earlier value, last the later.
func runDuplicates(t *testing.T, row *support.Row) tt.Duplicates {
	t.Helper()
	switch name := row.Named("duplicates"); name {
	case "":
		return tt.Reject
	case "first":
		return tt.FirstWins
	case "last":
		return tt.LastWins
	default:
		t.Fatalf("%s: duplicates is empty, first or last, not %q", row.Where(), name)
		return tt.Reject
	}
}

// transduceGrammar is the format a transduce row's grammar names, whose
// parser reads its document: one of the command's own, with a parser.
func transduceGrammar(t testing.TB, row *support.Row) *translate.Format {
	t.Helper()
	name := row.Named("grammar")
	grammar := translate.Named(name)
	if grammar == nil || !grammar.Parses() {
		t.Fatalf("%s: grammar names a format whose documents are parsed (%s), not %q", row.Where(), translate.ParserNames(), name)
	}
	return grammar
}

// runTransduce compiles program over the parser's rule passes and runs it
// over doc, parsed with grammar, as `dmog run --grammar` does, with the
// default limits; the program's output kind rides along, for the
// differential.
func runTransduce(program string, grammar *translate.Format, doc string, render Renderer, duplicates tt.Duplicates) (Output, runOutcome) {
	var output Output
	compiled, f := CompileWith(program, "transduce", InputParserEvents, routers, renderers)
	if f == nil && duplicates != tt.Reject {
		compiled, f = compiled.WithDuplicates(duplicates)
	}
	if f != nil {
		return output, runOutcome{fail: f}
	}
	var buffer bytes.Buffer
	sink, f := compiled.ParserSink(&buffer, render, tt.DefaultLimits(), tt.NewMetrics())
	if f != nil {
		return compiled.Output(), runOutcome{fail: f}
	}
	source, f := grammar.Passes(doc)
	if f != nil {
		return compiled.Output(), runOutcome{fail: f}
	}
	_, f = source.Limits(tt.DefaultLimits()).Metrics(tt.NewMetrics()).Run(sink)
	return compiled.Output(), runOutcome{out: buffer.String(), fail: f}
}

// collect collects a document's events into a value under a duplicate
// policy.
type collect struct{ builder *tt.DatumBuilder }

func (c *collect) Event(ev tt.Event) (tt.Flow, *tt.Fail) {
	if ev.Kind == tt.End {
		return tt.Continue, nil
	}
	if f := c.builder.Event(ev); f != nil {
		return tt.Continue, f
	}
	return tt.Continue, nil
}

// plainEvents is a value's events, a number's lexeme dropped.
func plainEvents(t testing.TB, value *tt.Datum) []tt.Event {
	t.Helper()
	recorder := &tt.Recorder{}
	if _, f := tt.WalkDatum(value, recorder); f != nil {
		t.Fatalf("a value walks: %v", f)
	}
	events := make([]tt.Event, len(recorder.Events))
	for i, ev := range recorder.Events {
		events[i] = ev.WithoutLexeme()
	}
	return events
}

// transduceDifferential is the differential rule of transduce.tsv: the events a
// row's program wrote, read back from its JSON, are the walk of the
// grammar's own value of the document, a number's lexeme aside. A
// document the grammar refuses has no value to walk, and a member the
// program streamed twice is kept once by the value: neither row has an
// oracle, and both pass here, held by their expected column alone.
func transduceDifferential(t testing.TB, grammar *translate.Format, doc, written string) error {
	t.Helper()
	value, f := grammar.Read(doc, tt.DefaultLimits())
	if f != nil {
		return nil
	}
	// The output read back through JSON's part streams a repeated member
	// twice, which the collection refuses.
	c := &collect{builder: tt.NewDatumBuilder(math.MaxInt, "max_capture_bytes", tt.Reject)}
	if f := jsonInto(written, tt.DefaultLimits(), tt.NewMetrics(), c); f != nil {
		if f.Code == tt.CodeDuplicateMember {
			return nil
		}
		return fmt.Errorf("the output %q does not read back as JSON: %v", written, f)
	}
	read, ok := c.builder.Take()
	if !ok {
		return fmt.Errorf("the output %q holds no value", written)
	}
	if memberOrderFree[grammar.ID()] {
		read, value = sortedMembers(read), sortedMembers(value)
	}
	a, b := plainEvents(t, &read), plainEvents(t, &value)
	if !reflect.DeepEqual(a, b) {
		return fmt.Errorf("the transducer's events are not the walk of the grammar's value\n  written: %v\n  walked:  %v", a, b)
	}
	return nil
}

// memberOrderFree is the formats whose Go reader gives an object's members
// in another order than the document's, so that the differential compares
// a row's events with the walk with each object's members in key order: Go
// reads a PGN database as typed values, a game's tags a map in sorted key
// order and a move's fields and annotations in the struct's order, and a
// stylesheet's nodes as plain maps, in sorted key order, where the part,
// as the Rust and TypeScript readers do, keeps the document's order
// (translate/translate_test.go's orderDivergent).
var memberOrderFree = map[string]bool{"css": true, "pgn": true}

// sortedMembers is a value with each object's members in key order.
func sortedMembers(d tt.Datum) tt.Datum {
	switch d.Kind {
	case tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			items[i] = sortedMembers(item)
		}
		d.Items = items
	case tt.DatumObject:
		members := make([]tt.Member, len(d.Members))
		for i, m := range d.Members {
			members[i] = tt.Member{Key: m.Key, Value: sortedMembers(m.Value)}
		}
		sort.SliceStable(members, func(i, j int) bool { return members[i].Key < members[j].Key })
		d.Members = members
	}
	return d
}

// TestSpecTransduce runs transduce.tsv: a program over a grammar's rule
// passes, run over a document in that grammar, the bytes it writes or the
// failure. A row's program is a transducer, its events the document's,
// which the differential holds it to where it answers JSON events. The
// source of rule passes needs no adapter, so every row runs in every
// build.
func TestSpecTransduce(t *testing.T) {
	spec, err := support.LoadSpec(filepath.Join(specDir(t), "transduce.tsv"), nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(spec.Rows) == 0 {
		t.Fatal("transduce.tsv holds no rows")
	}
	if got := strings.Join(spec.Rows[0].Header, " "); got != "input expected grammar doc render duplicates" {
		t.Fatalf("transduce.tsv's columns are %s", got)
	}
	r := support.Runner{
		ParseRow: func(input string, row *support.Row) (any, error) {
			grammar, doc, render := transduceGrammar(t, row), row.UnescNamed("doc"), runRender(t, row)
			duplicates := runDuplicates(t, row)
			output, o := runTransduce(input, grammar, doc, render, duplicates)
			if o.fail != nil {
				return nil, asErr(o.fail)
			}
			if output == OutputJsonEvents {
				if err := transduceDifferential(t, grammar, doc, o.out); err != nil {
					return nil, err
				}
			}
			return o.out, nil
		},
		ErrorCode: func(err error) string {
			if f, ok := failOf(err); ok {
				return runCode(f)
			}
			return err.Error()
		},
		ErrorPos: func(err error) (int, int, bool) {
			if f, ok := failOf(err); ok && f.Row > 0 {
				return int(f.Row), int(f.Column), true
			}
			return 0, 0, false
		},
	}
	for _, row := range spec.Rows {
		input := row.Unesc(0)
		t.Run(fmt.Sprintf("row %d", row.Line), func(t *testing.T) {
			if err := r.CheckRow(row, input, row.Col(1)); err != nil {
				t.Error(err)
			}
		})
	}
}
