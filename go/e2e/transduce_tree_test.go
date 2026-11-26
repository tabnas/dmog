// Copyright (c) 2026 tabnas, MIT License

package e2e

// transduce_tree_test.go: the library's transduce-tree and
// transduce-records, each part's one implementation
// (rs/tests/stdlib_test.rs): over transduce's fixtures, the generated
// documents, tabnas-csv's corpus and the documents below, a part whose
// spec is the grammar's part's writes the walk of the grammar's own value
// (transduce.tsv's rule, transduceDifferential), every part fails as the
// grammar refuses a document, and a :text rule's text and a records state
// are held to the limits.

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	. "github.com/tabnas/alchemy/go"
	"github.com/tabnas/alchemy/go/shared"
	"github.com/tabnas/dmog/go/translate"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
	tt "github.com/tabnas/transduce/go"
)

// treeSpecs are the specs the tree sweep runs: the json part's, and specs
// that take a close pass's value, hand a container on through a replaced
// rule, refuse a key that is not a string, announce a named key, yield a
// rule's text and name a member inside a rule's own container.
var treeSpecs = [][2]string{
	{"the json part's spec", "record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar 0))"},
	{"a close pass's value, keyed by name", "record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar :value))"},
	{"a container handed on through a replaced rule", "record\n    entry :list (record (entry :container :array))\n    entry :elem (record (entry :container :array))\n    entry :val (record (entry :scalar 0))"},
	{"a key whose token is not a string", "record\n    entry :val (record (entry :key 0) (entry :scalar 0))"},
	{"a named key on every value", "record\n    entry :list (record (entry :container :array))\n    entry :val (record (entry :key \"v\") (entry :scalar 0))"},
	{"a pair's value as its text", "record\n    entry :map (record (entry :container :object))\n    entry :pair (record (entry :key 0) (entry :text true))"},
	{"a container's own member and its text", "record\n    entry :list (record (entry :container :object) (entry :member \"items\") (entry :text true))\n    entry :pair (record (entry :member 0))"},
}

// partProgram compiles a part over the parser's rule passes: native is
// the library definition it runs, spec its spec.
func partProgram(t testing.TB, native, spec string) *Program {
	t.Helper()
	src := "def spec\n  " + spec + "\n\ndef export [input] (" + native + " spec input)\n"
	program, f := CompileWith(src, native+".alc", InputParserEvents, routers, renderers)
	if f != nil {
		t.Fatal(f)
	}
	return program
}

// runPasses drives one document's rule passes, parsed with make, into
// program, its output rendered as JSON: the output, or the failure.
func runPasses(program *Program, make func() *tabnas.Tabnas, text string) (string, *Fail) {
	return runPassesUnder(program, make, text, tt.DefaultLimits())
}

// runPassesUnder is runPasses under limits: what the program wrote, and
// the failure.
func runPassesUnder(program *Program, make func() *tabnas.Tabnas, text string, limits tt.Limits) (string, *Fail) {
	var out bytes.Buffer
	sink, f := program.ParserSink(&out, RenderJSON, limits, nil)
	if f != nil {
		return "", f
	}
	if _, f := tt.NewParserEventSource(make(), text).Run(sink); f != nil {
		return out.String(), f
	}
	return out.String(), nil
}

// isCatalogueCode is whether code is one of the shared codes.
func isCatalogueCode(code Code) bool {
	for _, c := range shared.AllCodes {
		if c == code {
			return true
		}
	}
	return false
}

// partOver runs a part over every document: where the grammar accepts the
// document and oracle says the part writes its tree, the output is the
// walk of the grammar's value (transduceDifferential, transduce.tsv's
// rule); where the grammar refuses it, the part fails, with the grammar's
// refusal or with a reason of its own it met first; and any other failure
// is one of the catalogue's, the part's own reason. The failures, for the
// test to report.
func partOver(t testing.TB, specName string, program *Program, format *translate.Format, make func() *tabnas.Tabnas, documents [][2]string, oracle bool) []string {
	t.Helper()
	var failures []string
	for _, doc := range documents {
		name := specName + ", " + doc[0]
		out, f := runPasses(program, make, doc[1])
		_, refusal := format.Read(doc[1], tt.DefaultLimits())
		refused := refusal != nil
		switch {
		case f != nil && refused:
			if !isCatalogueCode(f.Code) {
				failures = append(failures, fmt.Sprintf("%s: the grammar refuses the document, the part fails outside the catalogue: %v", name, f))
			}
		case f != nil:
			if oracle || !isCatalogueCode(f.Code) {
				failures = append(failures, fmt.Sprintf("%s: %v", name, f))
			}
		case refused:
			failures = append(failures, fmt.Sprintf("%s: wrote %q of a document the grammar refuses", name, out))
		case oracle:
			if err := transduceDifferential(t, format, doc[1], out); err != nil {
				failures = append(failures, fmt.Sprintf("%s: %v", name, err))
			}
		}
	}
	return failures
}

// fixtureDocuments is every transduce fixture with the extension, by name.
func fixtureDocuments(t testing.TB, ext string) [][2]string {
	t.Helper()
	dir := transduceFixtures(t)
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range entries {
		if filepath.Ext(e.Name()) == ext {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)
	var documents [][2]string
	for _, name := range names {
		text, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		documents = append(documents, [2]string{name, string(text)})
	}
	return documents
}

// The library's transduce-tree: over every JSON fixture of transduce's,
// the generated documents and the documents below, for the json part's
// spec and for specs that announce a named key, take a close pass's value,
// hand a container on through a replaced rule, name a member inside a
// rule's own container and yield a rule's text. The json part writes the
// walk of the grammar's value, and a document the grammar refuses fails as
// the grammar refuses it, for every spec.
func TestTheLibrarysTransduceTreeWritesEachDocumentsTree(t *testing.T) {
	documents := fixtureDocuments(t, ".json")
	for _, n := range []int{0, 1, 2, 50} {
		documents = append(documents, [2]string{fmt.Sprintf("recordsJSON(%d)", n), recordsJSON(n)})
	}
	for _, text := range []string{
		"1", "2.50", "-0", "1E+2", `"s"`, "null", "[]", "{}", "[[[]],[{}]]",
		`{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}`,
		`{"é":"日本","a\"b":"\u0000"}`,
		`{"a":1,"a":2}`,
		"[1,",
		`{"a" 1}`,
	} {
		documents = append(documents, [2]string{fmt.Sprintf("%q", text), text})
	}
	json := translate.Named("json")
	var failures []string
	compared := 0
	for _, spec := range treeSpecs {
		program := partProgram(t, "transduce-tree", spec[1])
		failures = append(failures, partOver(t, spec[0], program, json, jsonGrammar, documents, spec[0] == "the json part's spec")...)
		compared += len(documents)
	}
	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}
	if compared <= 100 {
		t.Errorf("%d documents compared", compared)
	}
	t.Logf("transduce-tree: %d documents written as their trees", compared)
}

// A :text rule's text under a small max_scalar_bytes: the library's
// string-join holds it at each of its joins (a pass's tokens together, the
// innermost text with them, a closed text with the text around it), so at
// each bound from one that refuses every text to one that refuses none the
// part writes the document (one its spec is sound for) or refuses it
// naming max_scalar_bytes, some of the refusals the texts' own, not the
// guard's; the widest bound refuses none.
func TestATextRulesTextIsHeldToMaxScalarBytes(t *testing.T) {
	textSpec := func(name string) string {
		for _, spec := range treeSpecs {
			if spec[0] == name && strings.Contains(spec[1], ":text") {
				return spec[1]
			}
		}
		t.Fatalf("no text spec %q", name)
		return ""
	}
	// Each spec over documents it is sound for: a pair's text is a scalar
	// after the pair's key, so no value may be an object (whose own
	// container would stand where the text goes), and a list's member and
	// text are announced inside the list's own object, so no pair, and no
	// list, may lie outside a flat array.
	specs := []struct {
		name, spec string
		documents  []string
	}{
		{"a pair's value as its text", textSpec("a pair's value as its text"), []string{
			`{"a":[10,20]}`,
			`{"a":"xyz","bb":[1,[22,333],"x"]}`,
			`{"a":1e21,"b":true,"c":null,"d":[]}`,
			`{"é":"日本"}`,
			`[1,[22,333],"x",{"é":"日本"}]`,
			`{"a":[1,2.50,"x"],"c":[],"e":1e21,"f":true}`,
		}},
		{"a container's own member and its text", textSpec("a container's own member and its text"), []string{
			`[1,2]`,
			`[]`,
			`[1,"x",true,null,2.50]`,
			`["日本","é"]`,
			`[1,2,3,4,5,6,7,8,9,10,11,12]`,
			`[-0,1E+2,"a,b"]`,
		}},
	}
	var failures []string
	textsRefused := 0
	for _, s := range specs {
		program := partProgram(t, "transduce-tree", s.spec)
		for _, text := range s.documents {
			for _, max := range []int{1, 3, 4, 5, 7, 8, 11, 16, 64} {
				limits := tt.DefaultLimits()
				limits.MaxScalarBytes = max
				out, f := runPassesUnder(program, jsonGrammar, text, limits)
				switch {
				case f == nil:
					if out == "" {
						t.Errorf("%s, %s, %d: wrote nothing", s.name, text, max)
					}
				case f.Code == CodeResourceLimitExceeded && f.Limit != nil && f.Limit.Name == "max_scalar_bytes":
					if max == 64 {
						t.Errorf("%s, %s: refused at the widest bound: %v", s.name, text, f)
					}
					if strings.HasPrefix(f.Message, "string-join:") {
						textsRefused++
					}
				default:
					failures = append(failures, fmt.Sprintf("%s, %s, max_scalar_bytes %d: %v", s.name, text, max, f))
				}
			}
		}
	}
	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}
	if textsRefused <= 10 {
		t.Errorf("%d texts refused at the bound", textsRefused)
	}
}

// csvDocuments is tabnas-csv's own test/spec corpus, transduce's CSV
// fixtures, the generated records and the documents below: what the
// records tests read.
func csvDocuments(t testing.TB) [][2]string {
	t.Helper()
	corpus := filepath.Join(repoRoot(t), "..", "csv", "test", "spec")
	files, err := support.LoadSpecDir(corpus, nil)
	if err != nil {
		t.Fatalf("tabnas-csv's corpus is vendored, at %s: %v", corpus, err)
	}
	var documents [][2]string
	for _, file := range files {
		for _, row := range file.Rows {
			documents = append(documents, [2]string{fmt.Sprintf("%s:%d", row.File, row.Line), row.UnescNamed("input")})
		}
	}
	documents = append(documents, fixtureDocuments(t, ".csv")...)
	for _, n := range []int{0, 1, 2, 50} {
		documents = append(documents, [2]string{fmt.Sprintf("recordsCSV(%d)", n), recordsCSV(n)})
	}
	for _, text := range []string{
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
	} {
		documents = append(documents, [2]string{fmt.Sprintf("%q", text), text})
	}
	return documents
}

// The library's transduce-records: over tabnas-csv's corpus, transduce's
// CSV fixtures, the generated records and documents that hold empty cells,
// short and long records, a header that repeats a name and documents the
// grammar refuses, for csv's part's spec and for specs without a header,
// without an empty group and with rules no pass names. csv's part writes
// the walk of the grammar's value (a header that names a column twice
// streams both cells, where the value keeps the last: no oracle there),
// and a document the grammar refuses fails as the grammar refuses it, for
// every spec.
func TestTheLibrarysTransduceRecordsWritesEachDocumentsRecords(t *testing.T) {
	specs := [][2]string{
		{"csv's part", "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :header true\n    entry :unnamed \"field~\""},
		{"no header, null for an empty cell", "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :fill null"},
		{"a header and no empty group", "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :header true"},
		{"rules no pass names", "record\n    entry :document \"doc\"\n    entry :record \"rec\"\n    entry :cell \"cell\"\n    entry :header true"},
	}
	documents := csvDocuments(t)
	csvFormat := translate.Named("csv")
	var failures []string
	compared := 0
	for _, spec := range specs {
		program := partProgram(t, "transduce-records", spec[1])
		failures = append(failures, partOver(t, spec[0], program, csvFormat, csvGrammar, documents, spec[0] == "csv's part")...)
		compared += len(documents)
	}
	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}
	if compared <= 200 {
		t.Errorf("%d documents compared", compared)
	}
	t.Logf("transduce-records: %d documents written as their records", compared)
}

// The state under a small max_metadata_bytes: the library's scans hold the
// header's names and a record's cells by the transduce measure, so at each
// bound from one that refuses the state before any pass to one that
// refuses none the part writes the document, the same bytes at every bound
// that lets it through, or refuses it naming max_metadata_bytes.
func TestTransduceRecordsStateIsHeldToMaxMetadataBytes(t *testing.T) {
	program := partProgram(t, "transduce-records", "record\n    entry :document \"csv\"\n    entry :record \"record\"\n    entry :cell \"val\"\n    entry :empty \"empty\"\n    entry :header true\n    entry :unnamed \"field~\"")
	text := "identifier,full name,balance\n1,Ann,2.50\n2,,\n3,Bob,7,extra\n"
	refused := 0
	written := ""
	for max := 8; max <= 220; max += 4 {
		limits := tt.DefaultLimits()
		limits.MaxMetadataBytes = max
		out, f := runPassesUnder(program, csvGrammar, text, limits)
		switch {
		case f == nil:
			if written == "" {
				written = out
			} else if out != written {
				t.Errorf("max_metadata_bytes %d: %q, where %q was written before", max, out, written)
			}
		case f.Code != CodeResourceLimitExceeded || f.Limit == nil || f.Limit.Name != "max_metadata_bytes":
			t.Errorf("max_metadata_bytes %d: %v", max, f)
		default:
			refused++
		}
	}
	if refused <= 10 {
		t.Errorf("%d bounds refused", refused)
	}
	if written == "" {
		t.Error("no bound let the document through")
	}
}
