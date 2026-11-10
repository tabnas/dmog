// Copyright (c) 2026 tabnas, MIT License

package translate_test

// translate_test.go: the cross product `dmog translate` answers for
// (rs/tests/translate_test.rs): every document of the corpora, read with
// its format's grammar, written in every format the command carries, read
// back with that format's grammar, and compared with the document's own
// value under the target's declared conventions (its loss list). A pair
// that fails to write, that writes a document its own grammar refuses, or
// that reads back as anything but the conventions say is a failure, and so
// is a corpus that shrinks.
//
// A pair whose target declares that it refuses the document is held to
// that refusal, its code and the start of its message, and counted: a
// schema-only target (one that writes a schema's tree with no embedding
// into it: C, CSS, PGN, proto and the grammar notations ABNF, EBNF and
// GBNF) refuses another format's tree, a grammar notation refuses a
// grammar it has no form for, naming the rule and what it met, and Semantic
// Versioning's embedding refuses a tree that is not a version. A refusal of
// another kind, or a document written where a refusal is declared, is a
// failure. The grammar notations share a schema, a grammar's rules in
// source order (grammar-rules), so each writes the others' documents.
//
// The corpora are the sibling checkouts': transduce's fixtures (aless's: a
// document of every format but C, CSS, expressions, PGN, proto, Semantic
// Versioning and the grammar notations, and more for YAML and ZON), the
// documents of JSONTestSuite every JSON parser must accept (jsonc's
// conformance pins), and the example grammars of the grammar notations'
// repositories, which the Rust suite reads in its release run. A fixture
// its own grammar refuses is no document, and is counted as one refused;
// so is a document of a format whose reader is a registered defect
// (readerDefects), counted apart. The Rust suite's cross product of every
// format's own fixture corpus, which it runs in release, stays Rust's.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"testing"
	"time"
	"unicode"
	"unicode/utf8"

	tabnasabnf "github.com/tabnas/abnf/go"
	alchemy "github.com/tabnas/alchemy/go"
	at "github.com/tabnas/alchemy/go/translate"
	"github.com/tabnas/dmog/go/translate"
	tabnasebnf "github.com/tabnas/ebnf/go"
	tabnasgbnf "github.com/tabnas/gbnf/go"
	tabnas "github.com/tabnas/parser/go"
	tt "github.com/tabnas/transduce/go"
)

// siblings is the directory the sibling checkouts are in: the repository
// root's parent.
func siblings(t testing.TB) string {
	t.Helper()
	dir, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

// formatOf is the format a fixture's extension names, by its id.
func formatOf(extension string) string {
	switch extension {
	case "json", "json5", "jsonc", "jsonic", "jsonl", "csv", "toml", "ini", "xml", "yaml", "zon":
		return extension
	case "md":
		return "markdown"
	case "rss", "atom":
		return "feed"
	case "c", "h":
		return "c"
	case "abnf", "ebnf", "gbnf":
		return extension
	}
	return ""
}

// extension is a file name's extension as Rust's Path::extension reads it:
// what follows the last dot, and none for a name that begins with its only
// dot.
func extension(name string) string {
	ext := filepath.Ext(name)
	if ext == name {
		return ""
	}
	return strings.TrimPrefix(ext, ".")
}

// document is one document of a corpus: its name, its format and its text.
type document struct {
	name, format, text string
}

// notation is what test/notation-samples.json holds: the inputs this
// repository gives the grammar notations' example grammars, the ones their
// repositories' own tests give them, by the name the cross product gives a
// document (Samples), and the pairs that recognise some of their samples
// otherwise across the lexing, each with exactly those samples, by the name
// the cross product gives a pair (Otherwise).
type notation struct {
	Samples   map[string][]string `json:"samples"`
	Otherwise map[string][]string `json:"otherwise"`
}

func notationSamples(t testing.TB) notation {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", "test", "notation-samples.json"))
	if err != nil {
		t.Fatalf("cannot read the samples: %v", err)
	}
	var file notation
	if err := json.Unmarshal(data, &file); err != nil {
		t.Fatalf("the samples are not JSON: %v", err)
	}
	return file
}

// jsonList is a list of samples as JSON, as test/notation-samples.json
// holds it.
func jsonList(list []string) string {
	var out strings.Builder
	enc := json.NewEncoder(&out)
	enc.SetEscapeHTML(false)
	if list == nil {
		list = []string{}
	}
	if err := enc.Encode(list); err != nil {
		return fmt.Sprintf("%q", list)
	}
	return strings.TrimSuffix(out.String(), "\n")
}

// notationExamples is the grammar notations' example grammars: each
// notation's directory, below its repository's checkout.
var notationExamples = []struct{ id, dir string }{
	{"abnf", filepath.Join("abnf", "ts", "test", "grammar")},
	{"ebnf", filepath.Join("ebnf", "ts", "test", "grammar")},
	{"gbnf", filepath.Join("gbnf", "test", "corpus")},
}

// corpus is every document of the corpora. A corpus may be held to the
// files whose names start with a prefix, or to those of one format (a
// grammar notation's examples sit beside a README).
func corpus(t testing.TB) []document {
	t.Helper()
	var docs []document
	corpora := []struct{ corpus, dir, prefix, only string }{
		{"transduce", filepath.Join(siblings(t), "transduce", "rs", "tests", "fixtures"), "", ""},
		{"JSONTestSuite", filepath.Join(siblings(t), "jsonc", "test", "JSONTestSuite", "test_parsing"), "y_", ""},
	}
	for _, n := range notationExamples {
		corpora = append(corpora, struct{ corpus, dir, prefix, only string }{n.id, filepath.Join(siblings(t), n.dir), "", n.id})
	}
	for _, c := range corpora {
		entries, err := os.ReadDir(c.dir)
		if err != nil {
			t.Fatalf("%s: cannot read %s: %v", c.corpus, c.dir, err)
		}
		for _, entry := range entries {
			name := entry.Name()
			if !strings.HasPrefix(name, c.prefix) {
				continue
			}
			id := formatOf(extension(name))
			if id == "" || (c.only != "" && c.only != id) {
				continue
			}
			data, err := os.ReadFile(filepath.Join(c.dir, name))
			if err != nil || !utf8.Valid(data) {
				continue
			}
			docs = append(docs, document{c.corpus + "/" + name, id, string(data)})
		}
	}
	return docs
}

func format(t testing.TB, id string) *translate.Format {
	t.Helper()
	f := translate.Named(id)
	if f == nil {
		t.Fatalf("%s is a format", id)
	}
	return f
}

// compiledKey is a composition's source, target and program.
type compiledKey struct {
	from, to   string
	program    string
	hasProgram bool
	whole      bool
}

// compiled is the compositions compiled so far, by source, target and
// program: one compiled composition serves every document of a pair.
var compiled = map[compiledKey]*translate.Compiled{}

// translateText translates text, read as from, into to, with program in
// front when there is one.
func translateText(from, to *translate.Format, text string, program *alchemy.Source) (string, *tt.Fail) {
	return translateAs(from, to, text, program, false)
}

// translateAs is translateText, read whole, as --whole reads it, where
// whole is asked.
func translateAs(from, to *translate.Format, text string, program *alchemy.Source, whole bool) (string, *tt.Fail) {
	request := &translate.Request{
		From:    from,
		To:      to,
		Options: at.DefaultOptions(),
		Program: program,
		Whole:   whole,
		Limits:  tt.DefaultLimits(),
	}
	key := compiledKey{from: from.ID(), to: to.ID(), whole: whole}
	if program != nil {
		key.program, key.hasProgram = program.Text, true
	}
	c, ok := compiled[key]
	if !ok {
		var f *tt.Fail
		if c, f = translate.Compile(request); f != nil {
			return "", f
		}
		compiled[key] = c
	}
	var out bytes.Buffer
	if f := translate.RunCompiled(request, c, text, &out, tt.NewMetrics()); f != nil {
		return "", f
	}
	return out.String(), nil
}

// ---------------------------------------------------------------------
// The readers registered as defective
// ---------------------------------------------------------------------

// readerDefects is the formats whose reader, as this command reads a
// document through transduce, does not build the tree the format's parts
// declare, for a defect of the format's module: each id with its defect
// (rs/tests/translate_test.rs's READER_DEFECTS, for this runtime's
// readers). The matrix reads no document of a registered format as a
// source, and counts the documents it leaves out; every format is still a
// target. TestARegisteredReaderDefectStillStands holds each entry to an
// example document, so an entry fails once its reader is repaired, and must
// then be deleted.
var readerDefects = map[string]string{
	"semver": "tabnassemver's reader builds a number past 2^53 - 1 as a *big.Int, which transduce's walker " +
		"writes as its fmt text, so 99999999999999999999.1.2 is refused by its own render; the Rust reader " +
		"keeps the digits, as the format's render takes them (its part's alchemy/render.alc). The format " +
		"is left out whole while it is registered",
}

// registeredDefect is whether documents of id are left out as sources for a
// registered reader defect.
func registeredDefect(id string) bool {
	_, ok := readerDefects[id]
	return ok
}

// TestARegisteredReaderDefectStillStands holds each registered reader
// defect to an example: a version past 2^53 - 1 does not survive its own
// round trip. Once a reader is repaired this fails, until its entry in
// readerDefects is deleted.
func TestARegisteredReaderDefectStillStands(t *testing.T) {
	for id, defect := range readerDefects {
		switch id {
		case "semver":
			example := "99999999999999999999.1.2"
			back, f := translateText(format(t, id), format(t, id), example, nil)
			if f == nil && back == example {
				t.Errorf("%s now keeps a version past 2^53 - 1: delete its entry (%s)", id, defect)
			}
		default:
			t.Fatalf("%s: a registered defect needs an example here", id)
		}
	}
}

// orderDivergent is the formats whose Go reader gives an object's members
// in another order than the Rust and TypeScript readers do, so that a
// translation from one writes them so: each id with an example document
// and its JSON as this command writes it. A plain map keeps no order, and
// transduce's walker gives its members in sorted key order (semver's
// version, a C syntax tree's nodes, and a PGN game's tags; a stylesheet's
// nodes were one until css's transducer part, which keeps the document's
// order, so css has no entry); a
// module's typed value is written in its fields' order, where the Rust and
// TypeScript readers keep the document's (a feed, and a PGN move's
// annotations). PGN's transducer part keeps the document's order, so its
// example is a document the part refuses, which --whole reads as the typed
// database's tree; every example is read so, in its reader's own order.
// Otherwise the values are the same, and the matrix, comparing values,
// cannot see the order;
// TestAReadersMemberOrderIsPinned holds each entry to its example. (A
// grammar spec was one until tabnas-bnf 0.1.30, whose Go serializer writes
// a spec's members in the order the compiler emits them, as the TypeScript
// and Rust compilers do, with no options.match.tokenOrder.)
var orderDivergent = map[string][2]string{
	"semver": {"1.2.3-rc.1", `{"build":[],"major":1,"minor":2,"patch":3,"prerelease":["rc",1]}`},
	"pgn": {"[White \"W\"]\n[Black \"B\"]\n\n1. e4 {a} (1. d4) {b} $1 1-0\n",
		`[{"tags":{"Black":"B","White":"W"},"moves":[{"san":"e4","piece":"P","to":"e4","number":1,"side":"w","nags":[1],` +
			`"comments":[{"kind":"brace","text":"a"},{"kind":"brace","text":"b"}],"variations":[{"moves":[{"san":"d4","piece":"P","to":"d4","number":1,"side":"w"}]}]}],"result":"1-0"}]`},
	"feed": {`<feed xmlns="http://www.w3.org/2005/Atom"><title>T</title><entry><title>A</title></entry></feed>`,
		`{"format":"atom","version":"1.0","title":{"type":"text","value":"T"},"entries":[{"title":{"type":"text","value":"A"}}]}`},
	"c": {"// c\n", `{"children":[],"kind":"translation_unit","span":{"col":1,"end":0,"line":1,"start":0},` +
		`"trivia":{"leading":[],"trailing":[]}}`},
}

// TestAReadersMemberOrderIsPinned holds each orderDivergent entry to its
// example. The Rust and TypeScript commands write semver's example as
// {"major":1,"minor":2,"patch":3,"prerelease":["rc",1],"build":[]}, css's
// and C's with each node's kind or type first, the game's tags as
// {"White":"W","Black":"B"} and the move's comments, variations and glyphs
// in the order they first appear, and the feed with its entries before its
// title.
func TestAReadersMemberOrderIsPinned(t *testing.T) {
	for id, c := range orderDivergent {
		got, f := translateAs(format(t, id), format(t, "json"), c[0], nil, true)
		if f != nil || strings.TrimSuffix(got, "\n") != c[1] {
			t.Errorf("%s now writes %q as %s (%v), not as registered: delete its entry if its members keep their order", id, c[0], got, f)
		}
	}
}

// ---------------------------------------------------------------------
// The refusals the targets declare
// ---------------------------------------------------------------------

// expectation is what a pair is held to: written, and read back under the
// target's conventions; refused as the target declares, with the code and
// the start of the message alchemy's composition or the target's part
// gives; or written unless the target refuses it so (unless), where the
// target declares that it refuses what it has no form for, naming what it
// met.
type expectation struct {
	written bool
	unless  bool
	code    tt.Code
	reason  string
}

// grammarNotation is whether a format is a grammar notation: one whose
// documents read as a grammar's rules in source order (grammar-rules,
// tabnas/bnf's), which each notation's render writes back.
func grammarNotation(f *translate.Format) bool {
	return f.Part.Schema == "grammar-rules"
}

// expect is what from's document, read as source, into to is held to. A
// schema-only target (one that writes from a tree, with a schema and no
// embed: C, CSS, PGN, proto and the grammar notations) refuses a tree of
// another schema before any output, as alchemy's composition declares; a
// grammar notation's render writes a grammar's rules, any notation's,
// unless it has no form for something in them, which it refuses naming the
// rule and what it met, as its loss list declares (prose, a negated class
// in ABNF, the engine's own tokens in GBNF, a start the grammar does not
// define, ...); Semantic Versioning's embedding
// refuses a tree that is not a version, as its part declares; and expr's
// render refuses a list led by an object whose src its reader would take
// for the list's operator, where that is no default operator's source text
// (a C syntax tree's token), as its loss list declares (exprRefuses).
// Every other pair is written, Markdown's table among them: it writes from
// records, which any tree makes.
func expect(from, to *translate.Format, source *tt.Datum) expectation {
	target := to.Part
	foreign := target.Writes == at.ShapeTree && target.Schema != "" && from.Part.Schema != target.Schema
	switch {
	case foreign && target.Embed == nil:
		return expectation{code: tt.CodeTargetValueUnrepresentable,
			reason: "schema_only: " + target.ID + " writes a " + target.Schema + " tree, "}
	case foreign && to.ID() == "semver" && !semverVersion(source):
		return expectation{code: tt.CodeTargetValueUnrepresentable, reason: "the document is not a version: "}
	case to.ID() == "expr" && exprRefuses(*source):
		return expectation{code: tt.CodeTargetValueUnrepresentable, reason: "the object at "}
	case grammarNotation(to):
		return expectation{written: true, unless: true, code: tt.CodeTargetValueUnrepresentable,
			reason: "the grammar cannot be written as " + strings.ToUpper(to.ID()) + ": "}
	}
	return expectation{written: true}
}

// allDigits is whether s is digits alone, and not empty.
func allDigits(s string) bool {
	if s == "" {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return true
}

// writtenDigits is the text a number is written with when it is digits
// alone: its lexeme, or the text JSON writes for it, ECMAScript's, which
// spells a whole number below 10^21 with its digits.
func writtenDigits(d *tt.Datum) (string, bool) {
	if d.Kind != tt.DatumNumber {
		return "", false
	}
	text := d.Lexeme
	if !d.HasLexeme {
		v := d.Value
		if math.IsInf(v, 0) || math.IsNaN(v) || v != math.Trunc(v) || math.Signbit(v) || v >= 1e21 {
			return "", false
		}
		text = strconv.FormatFloat(v, 'f', 0, 64)
	}
	return text, allDigits(text)
}

// digitString is whether s is digits with no leading zero, but for 0
// itself.
func digitString(s string) bool {
	return allDigits(s) && (s == "0" || s[0] != '0')
}

// semverIdentifier is whether a value is a prerelease (prerelease) or
// build identifier: a number written in digits, or a string of 0-9, A-Z,
// a-z and -, not empty, which for a prerelease is no number of digits that
// begins with a zero.
func semverIdentifier(d *tt.Datum, prerelease bool) bool {
	switch d.Kind {
	case tt.DatumNumber:
		_, ok := writtenDigits(d)
		return ok
	case tt.DatumString:
		s := d.Text
		if s == "" {
			return false
		}
		for i := 0; i < len(s); i++ {
			c := s[i]
			if !(c == '-' || ('0' <= c && c <= '9') || ('A' <= c && c <= 'Z') || ('a' <= c && c <= 'z')) {
				return false
			}
		}
		return !(prerelease && allDigits(s) && !digitString(s))
	}
	return false
}

// semverVersion is whether a value is a version as Semantic Versioning's
// embedding takes one (its part's alchemy/embed.alc): an object whose
// major, minor and patch are whole numbers written in digits (a number
// whose text is digits alone, or a string of digits with no leading zero),
// and whose prerelease and build, where it has them, are null, the empty
// string, or a list of identifiers or one string of them joined by dots.
func semverVersion(d *tt.Datum) bool {
	if d.Kind != tt.DatumObject {
		return false
	}
	core := func(key string) bool {
		v, ok := d.Get(key)
		switch {
		case !ok:
			return false
		case v.Kind == tt.DatumNumber:
			_, digits := writtenDigits(v)
			return digits
		}
		return v.Kind == tt.DatumString && digitString(v.Text)
	}
	identifiers := func(key string, prerelease bool) bool {
		v, ok := d.Get(key)
		switch {
		case !ok || v.Kind == tt.DatumNull:
			return true
		case v.Kind == tt.DatumString:
			if v.Text == "" {
				return true
			}
			for _, id := range strings.Split(v.Text, ".") {
				item := tt.StringDatum(id)
				if !semverIdentifier(&item, prerelease) {
					return false
				}
			}
			return true
		case v.Kind == tt.DatumArray:
			for i := range v.Items {
				if !semverIdentifier(&v.Items[i], prerelease) {
					return false
				}
			}
			return true
		}
		return false
	}
	return core("major") && core("minor") && core("patch") && identifiers("prerelease", true) && identifiers("build", false)
}

// ---------------------------------------------------------------------
// Values compared as the conventions compare them
// ---------------------------------------------------------------------

// same is whether two values are the same: numbers by value (NaN is NaN),
// objects by their members whatever their order, arrays in order.
func same(a, b *tt.Datum) bool {
	switch {
	case a.Kind == tt.DatumNumber && b.Kind == tt.DatumNumber:
		return a.Value == b.Value || (math.IsNaN(a.Value) && math.IsNaN(b.Value))
	case a.Kind == tt.DatumArray && b.Kind == tt.DatumArray:
		if len(a.Items) != len(b.Items) {
			return false
		}
		for i := range a.Items {
			if !same(&a.Items[i], &b.Items[i]) {
				return false
			}
		}
		return true
	case a.Kind == tt.DatumObject && b.Kind == tt.DatumObject:
		if len(a.Members) != len(b.Members) {
			return false
		}
		for i := range a.Members {
			w, ok := b.Get(a.Members[i].Key)
			if !ok || !same(&a.Members[i].Value, w) {
				return false
			}
		}
		return true
	}
	return a.Equal(*b)
}

// mapNonFinite is the value with every number that is not finite
// replaced.
func mapNonFinite(d tt.Datum, f func(float64) tt.Datum) tt.Datum {
	switch {
	case d.Kind == tt.DatumNumber && (math.IsInf(d.Value, 0) || math.IsNaN(d.Value)):
		return f(d.Value)
	case d.Kind == tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			items[i] = mapNonFinite(item, f)
		}
		return tt.ArrayDatum(items...)
	case d.Kind == tt.DatumObject:
		members := make([]tt.Member, len(d.Members))
		for i, m := range d.Members {
			members[i] = tt.Member{Key: m.Key, Value: mapNonFinite(m.Value, f)}
		}
		return tt.ObjectDatum(members...)
	}
	return d
}

func wrapObject(d tt.Datum, key string) tt.Datum {
	if d.Kind == tt.DatumObject {
		return d
	}
	return tt.ObjectDatum(tt.Member{Key: key, Value: d})
}

func wrapArray(d tt.Datum) tt.Datum {
	if d.Kind == tt.DatumArray {
		return d
	}
	return tt.ArrayDatum(d)
}

// withoutNulls is TOML's conventions: no null (a member whose value is
// null is not written, a null element is skipped).
func withoutNulls(d tt.Datum) tt.Datum {
	switch d.Kind {
	case tt.DatumArray:
		items := []tt.Datum{}
		for _, item := range d.Items {
			if item.Kind != tt.DatumNull {
				items = append(items, withoutNulls(item))
			}
		}
		return tt.ArrayDatum(items...)
	case tt.DatumObject:
		members := []tt.Member{}
		for _, m := range d.Members {
			if m.Value.Kind != tt.DatumNull {
				members = append(members, tt.Member{Key: m.Key, Value: withoutNulls(m.Value)})
			}
		}
		return tt.ObjectDatum(members...)
	}
	return d
}

// rustFloat is the grammar Rust's f64 parse takes: a sign at most, then
// inf, infinity or nan in any case, or decimal digits with a point and an
// exponent at most; no blanks, underscores or hexadecimal.
var rustFloat = regexp.MustCompile(`^[+-]?(?:(?i:inf|infinity|nan)|(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?)$`)

// parseF64 is text read as Rust's `str::parse::<f64>` reads it, which the
// Rust suite compares a cell's text with: an overflow is an infinity, not
// a failure.
func parseF64(text string) (float64, bool) {
	if !rustFloat.MatchString(text) {
		return 0, false
	}
	value, err := strconv.ParseFloat(text, 64)
	if err != nil && !math.IsInf(value, 0) {
		return math.NaN(), true
	}
	return value, true
}

// cell is the text a cell is written as in CSV and Markdown: a string as
// it is, a number by its value (compared as one), a non-finite one by its
// word, a boolean by its name, null and an absent member as the empty
// field, a container as its compact JSON text, compared as the value that
// text reads back as: a route over the parser's rule passes writes a
// number inside it by its lexeme (55.0) where the materialized value the
// expectation is made of spells it by its value (55), one number either way.
type cell struct {
	text     string
	number   float64
	isNumber bool
	nested   bool
}

func cellOf(d *tt.Datum) cell {
	switch {
	case d == nil || d.Kind == tt.DatumNull:
		return cell{}
	case d.Kind == tt.DatumBool:
		return cell{text: strconv.FormatBool(d.Bool)}
	case d.Kind == tt.DatumNumber && math.IsNaN(d.Value):
		return cell{text: "NaN"}
	case d.Kind == tt.DatumNumber && math.IsInf(d.Value, 1):
		return cell{text: "Infinity"}
	case d.Kind == tt.DatumNumber && math.IsInf(d.Value, -1):
		return cell{text: "-Infinity"}
	case d.Kind == tt.DatumNumber:
		return cell{number: d.Value, isNumber: true}
	case d.Kind == tt.DatumString:
		return cell{text: d.Text}
	}
	return cell{text: d.String(), nested: true}
}

func (c cell) is(got string) bool {
	if c.isNumber {
		g, ok := parseF64(got)
		return ok && g == c.number
	}
	if c.text == got {
		return true
	}
	if !c.nested {
		return false
	}
	want, ok := readJSON(c.text)
	if !ok {
		return false
	}
	have, ok := readJSON(got)
	return ok && sameJSON(want, have)
}

// sameJSON is whether two values readJSON read are the same: numbers by
// value, objects by their members whatever their order, arrays in order.
func sameJSON(a, b any) bool {
	switch a := a.(type) {
	case json.Number:
		bn, ok := b.(json.Number)
		if !ok {
			return false
		}
		x, errA := a.Float64()
		y, errB := bn.Float64()
		return errA == nil && errB == nil && x == y
	case []any:
		bs, ok := b.([]any)
		if !ok || len(a) != len(bs) {
			return false
		}
		for i := range a {
			if !sameJSON(a[i], bs[i]) {
				return false
			}
		}
		return true
	case map[string]any:
		bm, ok := b.(map[string]any)
		if !ok || len(a) != len(bm) {
			return false
		}
		for k, v := range a {
			w, ok := bm[k]
			if !ok || !sameJSON(v, w) {
				return false
			}
		}
		return true
	}
	return a == b
}

func (c cell) String() string {
	if c.isNumber {
		return strconv.FormatFloat(c.number, 'g', -1, 64)
	}
	return strconv.Quote(c.text)
}

// table is the table the inferred binding makes of a value: the rows are
// the root array's elements (a root of another kind is one row), the
// columns the first row's (an object's keys, an array's positions, or one
// value column for a scalar), each row's cell found by the column's path.
func table(d tt.Datum) ([]string, [][]cell) {
	rows := wrapArray(d).Items
	// A column's path: a member's key, an element's index, or the row
	// itself.
	const (
		byKey = iota
		byIndex
		itself
	)
	type column struct {
		label string
		path  int
		key   string
		index int
	}
	var columns []column
	if len(rows) > 0 {
		switch first := rows[0]; first.Kind {
		case tt.DatumObject:
			for _, m := range first.Members {
				columns = append(columns, column{label: m.Key, path: byKey, key: m.Key})
			}
		case tt.DatumArray:
			for i := range first.Items {
				columns = append(columns, column{label: strconv.Itoa(i), path: byIndex, index: i})
			}
		default:
			columns = append(columns, column{label: "value", path: itself})
		}
	}
	cells := make([][]cell, len(rows))
	for r := range rows {
		row := &rows[r]
		for _, c := range columns {
			var found *tt.Datum
			switch c.path {
			case byKey:
				if row.Kind == tt.DatumObject {
					found, _ = row.Get(c.key)
				}
			case byIndex:
				if row.Kind == tt.DatumArray && c.index < len(row.Items) {
					found = &row.Items[c.index]
				}
			default:
				found = row
			}
			cells[r] = append(cells[r], cellOf(found))
		}
	}
	labels := make([]string, len(columns))
	for i, c := range columns {
		labels[i] = c.label
	}
	return labels, cells
}

// checkRecords is whether a read-back table (an array of objects keyed by
// label, every value a string) is the table the inferred binding makes of
// source, with each cell's text passed through cellText first (Markdown's
// normalisation of what it writes).
func checkRecords(source, back tt.Datum, cellText func(string) string) error {
	labels, rows := table(source)
	if back.Kind != tt.DatumArray {
		return fmt.Errorf("read back as %s, not an array of records", back)
	}
	backRows := back.Items
	if len(labels) == 0 {
		if len(backRows) == 0 {
			return nil
		}
		return fmt.Errorf("a table of no columns read back as %s", back)
	}
	if len(backRows) != len(rows) {
		return fmt.Errorf("%d rows read back, %d written: %s", len(backRows), len(rows), back)
	}
	for i, row := range rows {
		got := backRows[i]
		if got.Kind != tt.DatumObject {
			return fmt.Errorf("row %d read back as %s", i, got)
		}
		for j, label := range labels {
			// A label is a header cell, written and read back as any cell.
			text := ""
			if v, ok := got.Get(cellText(label)); ok {
				switch v.Kind {
				case tt.DatumString:
					text = v.Text
				case tt.DatumNull:
				default:
					text = v.String()
				}
			}
			expected := row[j]
			if !expected.isNumber {
				expected.text = cellText(expected.text)
			}
			if !expected.is(text) {
				return fmt.Errorf("row %d, column %q: read back %q, where %s was written", i, label, text, expected)
			}
		}
	}
	return nil
}

// markdownCell is Markdown's normalisation of a written cell: a line break
// is a space, a U+0000 is U+FFFD, and the whitespace at either end is not
// kept, as the reader trims it: what JavaScript's trim takes, which is
// Unicode's whitespace without U+0085 and with U+FEFF.
func markdownCell(text string) string {
	text = strings.ReplaceAll(text, "\r\n", " ")
	text = strings.NewReplacer("\n", " ", "\r", " ", "\x00", "\uFFFD").Replace(text)
	return strings.TrimFunc(text, func(c rune) bool {
		return (unicode.IsSpace(c) && c != '\u0085') || c == '\uFEFF'
	})
}

// ---------------------------------------------------------------------
// The cross product
// ---------------------------------------------------------------------

// iniSame is whether an INI document read back (back) is what INI's
// conventions make of expected: an object is a section (or the root) and
// an array of scalars is key[] lines, each read back as itself; a number
// reads back as its text, and one that is not finite as its word; a
// container INI has no place for (inside an array, an empty array, or
// under a key no header can spell) reads back as its compact JSON text, a
// string; true, false and null read back as themselves, and a string as
// itself.
func iniSame(expected, back *tt.Datum) bool {
	switch {
	case expected.Kind == tt.DatumObject && back.Kind == tt.DatumObject:
		if len(expected.Members) != len(back.Members) {
			return false
		}
		for i := range expected.Members {
			k := expected.Members[i].Key
			w, ok := back.Get(k)
			if !ok {
				w, ok = back.Get(strings.TrimFunc(k, unicode.IsSpace))
			}
			if !ok || !iniSame(&expected.Members[i].Value, w) {
				return false
			}
		}
		return true
	case expected.Kind == tt.DatumArray && back.Kind == tt.DatumArray && len(expected.Items) > 0:
		if len(expected.Items) != len(back.Items) {
			return false
		}
		for i := range expected.Items {
			if !iniItem(&expected.Items[i], &back.Items[i]) {
				return false
			}
		}
		return true
	case expected.Kind == tt.DatumNumber && back.Kind == tt.DatumString:
		return numberTextIs(expected.Value, back.Text)
	case expected.IsContainer() && back.Kind == tt.DatumString:
		return jsonTextIs(expected, back.Text)
	}
	return same(expected, back)
}

// iniItem is an array item: a scalar as iniSame reads it, a container as
// its JSON text.
func iniItem(expected, back *tt.Datum) bool {
	if expected.IsContainer() && back.Kind == tt.DatumString {
		return jsonTextIs(expected, back.Text)
	}
	return iniSame(expected, back)
}

// numberTextIs is whether text spells the number value: its digits, or
// the word of one that is not finite.
func numberTextIs(value float64, text string) bool {
	switch text {
	case "Infinity":
		return math.IsInf(value, 1)
	case "-Infinity":
		return math.IsInf(value, -1)
	case "NaN":
		return math.IsNaN(value)
	}
	n, ok := parseF64(text)
	return ok && n == value
}

// readJSON is text read as one JSON value, as the Rust suite reads it with
// serde_json: numbers kept as text, nothing after the value but blanks, and
// a number past the largest double refused.
func readJSON(text string) (any, bool) {
	dec := json.NewDecoder(strings.NewReader(text))
	dec.UseNumber()
	var value any
	if err := dec.Decode(&value); err != nil {
		return nil, false
	}
	if _, err := dec.Token(); err != io.EOF {
		return nil, false
	}
	var finite func(any) bool
	finite = func(v any) bool {
		switch v := v.(type) {
		case json.Number:
			f, err := strconv.ParseFloat(v.String(), 64)
			return err == nil && !math.IsInf(f, 0)
		case []any:
			for _, item := range v {
				if !finite(item) {
					return false
				}
			}
		case map[string]any:
			for _, item := range v {
				if !finite(item) {
					return false
				}
			}
		}
		return true
	}
	return value, finite(value)
}

// jsonTextIs is whether text is the compact JSON text of container, read
// back as JSON and compared as values, a number that is not finite
// matching null or its word.
func jsonTextIs(container *tt.Datum, text string) bool {
	value, ok := readJSON(text)
	if !ok {
		return false
	}
	var matches func(d *tt.Datum, j any) bool
	matches = func(d *tt.Datum, j any) bool {
		switch j := j.(type) {
		case nil:
			return d.Kind == tt.DatumNull || d.Kind == tt.DatumNumber
		case bool:
			return d.Kind == tt.DatumBool && d.Bool == j
		case json.Number:
			f, err := strconv.ParseFloat(j.String(), 64)
			return d.Kind == tt.DatumNumber && err == nil && f == d.Value
		case string:
			switch d.Kind {
			case tt.DatumNumber:
				return numberTextIs(d.Value, j)
			case tt.DatumString:
				return d.Text == j
			}
			return false
		case []any:
			if d.Kind != tt.DatumArray || len(d.Items) != len(j) {
				return false
			}
			for i := range d.Items {
				if !matches(&d.Items[i], j[i]) {
					return false
				}
			}
			return true
		case map[string]any:
			if d.Kind != tt.DatumObject || len(d.Members) != len(j) {
				return false
			}
			for i := range d.Members {
				w, ok := j[d.Members[i].Key]
				if !ok || !matches(&d.Members[i].Value, w) {
					return false
				}
			}
			return true
		}
		return false
	}
	return matches(container, value)
}

// zonBigDigits is whether a string spells an integer as ZON's reader
// writes a big integer's digits, which is when the render writes a lone
// $big as the integer itself: a minus sign at most, and first, then 0 or
// digits that do not begin with 0, but not -0.
func zonBigDigits(s string) bool {
	digits := strings.TrimPrefix(s, "-")
	if digits == "" || s == "-0" {
		return false
	}
	for i := 0; i < len(digits); i++ {
		if digits[i] < '0' || digits[i] > '9' {
			return false
		}
	}
	return digits == "0" || digits[0] != '0'
}

// zonBig is the integer an object whose only member is $big spells, when
// its value is a big integer's digits: ZON's render writes that object as
// the integer, and its reader builds an integer no double holds exactly as
// that object.
func zonBig(d *tt.Datum) (tt.Datum, bool) {
	if d.Kind != tt.DatumObject || len(d.Members) != 1 {
		return tt.Datum{}, false
	}
	v, ok := d.Get("$big")
	if !ok || v.Kind != tt.DatumString || !zonBigDigits(v.Text) {
		return tt.Datum{}, false
	}
	value, ok := parseF64(v.Text)
	if !ok {
		return tt.Datum{}, false
	}
	return tt.NumberDatumLexeme(value, v.Text), true
}

// zonReading is what ZON's conventions make of a value it is given: an
// empty struct reads back as an empty tuple, and a lone $big holding a big
// integer's digits as that integer.
func zonReading(d tt.Datum) tt.Datum {
	switch d.Kind {
	case tt.DatumObject:
		if len(d.Members) == 0 {
			return tt.ArrayDatum()
		}
		if big, ok := zonBig(&d); ok {
			return big
		}
		members := make([]tt.Member, len(d.Members))
		for i, m := range d.Members {
			members[i] = tt.Member{Key: m.Key, Value: zonReading(m.Value)}
		}
		return tt.ObjectDatum(members...)
	case tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			items[i] = zonReading(item)
		}
		return tt.ArrayDatum(items...)
	}
	return d
}

// zonName is a field name as ZON's render wrote it, read back by the
// declared reverse of its convention: $empty is the empty name; $$ and a
// rest is $ and the rest; $json: and a text is the string the text spells
// as a double-quoted JSON string; any other name is as it is.
func zonName(written string) string {
	switch {
	case written == "$empty":
		return ""
	case strings.HasPrefix(written, "$$"):
		return "$" + written[2:]
	case strings.HasPrefix(written, "$json:"):
		var s string
		if err := json.Unmarshal([]byte(written[len("$json:"):]), &s); err != nil {
			return written
		}
		return s
	}
	return written
}

// zonBack is what ZON's reader made of a document its render wrote, as
// the value it was: a lone $big (the reader's big integer) is the integer,
// and every field name reads back by the reverse of the convention that
// wrote it.
func zonBack(d tt.Datum) tt.Datum {
	if big, ok := zonBig(&d); ok {
		return big
	}
	switch d.Kind {
	case tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			items[i] = zonBack(item)
		}
		return tt.ArrayDatum(items...)
	case tt.DatumObject:
		members := make([]tt.Member, len(d.Members))
		for i, m := range d.Members {
			members[i] = tt.Member{Key: zonName(m.Key), Value: zonBack(m.Value)}
		}
		return tt.ObjectDatum(members...)
	}
	return d
}

// exprOperators is the default operators' source texts, which expr's
// render writes as the operator (`(` a group): an operation is a list whose
// first element is one, or an object whose src is one.
var exprOperators = map[string]bool{"+": true, "-": true, "*": true, "/": true, "%": true, "(": true}

// exprRefuses is whether expr's render refuses a tree (its loss list): it
// holds a list whose first element is an object with a src member a
// simplifier would read as the list's operator, anything but the empty
// string, false, null or zero, which is no default operator's source text,
// as the tokens a C syntax tree puts first in a node's children are.
func exprRefuses(d tt.Datum) bool {
	switch d.Kind {
	case tt.DatumArray:
		if len(d.Items) > 0 && d.Items[0].Kind == tt.DatumObject {
			if src, ok := d.Items[0].Get("src"); ok {
				read := true
				switch src.Kind {
				case tt.DatumNull:
					read = false
				case tt.DatumBool:
					read = src.Bool
				case tt.DatumString:
					read = src.Text != "" && !exprOperators[src.Text]
				case tt.DatumNumber:
					read = src.Value != 0 || (src.HasLexeme && strings.HasPrefix(src.Lexeme, "-"))
				}
				if read {
					return true
				}
			}
		}
		for _, item := range d.Items {
			if exprRefuses(item) {
				return true
			}
		}
	case tt.DatumObject:
		for _, m := range d.Members {
			if exprRefuses(m.Value) {
				return true
			}
		}
	}
	return false
}

// exprSimplify is a tree as expr's reader reads one back (its simplified
// tree, the form its shared fixtures hold): a list whose first element is
// an object whose src is a default operator's source text has that text in
// the object's place, as expr's loss list declares. Any other such src its
// render refuses (exprRefuses), so no tree holding one is read back.
func exprSimplify(d tt.Datum) tt.Datum {
	switch d.Kind {
	case tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			if i == 0 && item.Kind == tt.DatumObject {
				if src, ok := item.Get("src"); ok && src.Kind == tt.DatumString && exprOperators[src.Text] {
					items[i] = tt.StringDatum(src.Text)
					continue
				}
			}
			items[i] = exprSimplify(item)
		}
		return tt.ArrayDatum(items...)
	case tt.DatumObject:
		members := make([]tt.Member, len(d.Members))
		for i, m := range d.Members {
			members[i] = tt.Member{Key: m.Key, Value: exprSimplify(m.Value)}
		}
		return tt.ObjectDatum(members...)
	}
	return d
}

// exprReading is what expr's conventions make of a tree it is given (its
// loss list): an operator's description reads back as its source text; a
// negative number, written with its sign, as the operator - applied to its
// magnitude; and a number that is not finite as the string Infinity,
// -Infinity or NaN.
func exprReading(d tt.Datum) tt.Datum {
	var numbers func(d tt.Datum) tt.Datum
	numbers = func(d tt.Datum) tt.Datum {
		switch d.Kind {
		case tt.DatumNumber:
			switch {
			case math.IsNaN(d.Value):
				return tt.StringDatum("NaN")
			case math.IsInf(d.Value, 1):
				return tt.StringDatum("Infinity")
			case math.IsInf(d.Value, -1):
				return tt.StringDatum("-Infinity")
			}
			signed := math.Signbit(d.Value)
			if d.HasLexeme {
				signed = strings.HasPrefix(d.Lexeme, "-")
			}
			if signed {
				return tt.ArrayDatum(tt.StringDatum("-"), tt.NumberDatum(math.Abs(d.Value)))
			}
			return d
		case tt.DatumArray:
			items := make([]tt.Datum, len(d.Items))
			for i, item := range d.Items {
				items[i] = numbers(item)
			}
			return tt.ArrayDatum(items...)
		case tt.DatumObject:
			members := make([]tt.Member, len(d.Members))
			for i, m := range d.Members {
				members[i] = tt.Member{Key: m.Key, Value: numbers(m.Value)}
			}
			return tt.ObjectDatum(members...)
		}
		return d
	}
	return numbers(exprSimplify(d))
}

// maxSafeInteger is the largest integer every runtime's reader of a
// version keeps as a number, 2^53 - 1; Semantic Versioning's Rust reader
// keeps one past it as its digits.
const maxSafeInteger = 9007199254740991

// semverNumber is a version's number as Semantic Versioning's reader
// builds it from its digits: a number up to 2^53 - 1, and its digits past
// it.
func semverNumber(digits string) tt.Datum {
	value, err := strconv.ParseFloat(digits, 64)
	if err == nil && value <= maxSafeInteger {
		return tt.NumberDatum(value)
	}
	return tt.StringDatum(digits)
}

// semverReading is what a version reads back as, by Semantic Versioning's
// loss list: its major, minor and patch, each the number its digits make;
// its prerelease and build as lists, empty where they are absent, null or
// empty, one string split at its dots; a prerelease identifier of digits as
// the number they make and a build identifier as its text; and no other
// member.
func semverReading(d tt.Datum) tt.Datum {
	text := func(v *tt.Datum) string {
		if v.Kind == tt.DatumString {
			return v.Text
		}
		digits, _ := writtenDigits(v)
		return digits
	}
	identifiers := func(key string, prerelease bool) tt.Datum {
		var items []tt.Datum
		if v, ok := d.Get(key); ok {
			switch {
			case v.Kind == tt.DatumString && v.Text != "":
				for _, id := range strings.Split(v.Text, ".") {
					items = append(items, tt.StringDatum(id))
				}
			case v.Kind == tt.DatumArray:
				items = v.Items
			}
		}
		out := make([]tt.Datum, len(items))
		for i := range items {
			id := text(&items[i])
			if prerelease && allDigits(id) {
				out[i] = semverNumber(id)
			} else {
				out[i] = tt.StringDatum(id)
			}
		}
		return tt.ArrayDatum(out...)
	}
	core := func(key string) tt.Datum {
		v, _ := d.Get(key)
		return semverNumber(text(v))
	}
	return tt.ObjectDatum(
		tt.Member{Key: "major", Value: core("major")},
		tt.Member{Key: "minor", Value: core("minor")},
		tt.Member{Key: "patch", Value: core("patch")},
		tt.Member{Key: "prerelease", Value: identifiers("prerelease", true)},
		tt.Member{Key: "build", Value: identifiers("build", false)},
	)
}

// feedRender is the id, and an author's uri, the feed render supplies
// where a feed has none.
const feedRender = "tag:tabnas.dev,2026:feed-render"

// feedEpoch is the date the feed render supplies where a feed or an entry
// has none, the one its embedding gives a plain tree.
const feedEpoch = "1970-01-01T00:00:00Z"

// digitsOf is whether s is n decimal digits.
func digitsOf(n int, s string) bool {
	return len(s) == n && (n == 0 || allDigits(s))
}

// twoDigits is whether s is two digits from lo to hi.
func twoDigits(lo, hi int, s string) bool {
	if !digitsOf(2, s) {
		return false
	}
	n, _ := strconv.Atoi(s)
	return lo <= n && n <= hi
}

// rfc3339 is whether s is an RFC 3339 date-time with the upper-case T and
// Z Atom asks for, as the feed render reads one: a full date; a time, whose
// seconds may have a fraction; and Z or an offset.
func rfc3339(s string) bool {
	second := func(sec string) bool {
		p := strings.Split(sec, ".")
		switch len(p) {
		case 1:
			return twoDigits(0, 60, sec)
		case 2:
			return twoDigits(0, 60, p[0]) && allDigits(p[1])
		}
		return false
	}
	clock := func(t string) bool {
		p := strings.Split(t, ":")
		return len(p) == 3 && twoDigits(0, 23, p[0]) && twoDigits(0, 59, p[1]) && second(p[2])
	}
	offset := func(o string) bool {
		p := strings.Split(o, ":")
		return len(p) == 2 && twoDigits(0, 23, p[0]) && twoDigits(0, 59, p[1])
	}
	parts := strings.Split(s, "T")
	if len(parts) != 2 {
		return false
	}
	d := strings.Split(parts[0], "-")
	if !(len(d) == 3 && digitsOf(4, d[0]) && twoDigits(1, 12, d[1]) && twoDigits(1, 31, d[2])) {
		return false
	}
	rest := parts[1]
	switch z := strings.Split(rest, "Z"); len(z) {
	case 2:
		return z[1] == "" && clock(z[0])
	case 1:
		switch plus := strings.Split(rest, "+"); len(plus) {
		case 2:
			return clock(plus[0]) && offset(plus[1])
		case 1:
			minus := strings.Split(rest, "-")
			return len(minus) == 2 && clock(minus[0]) && offset(minus[1])
		}
	}
	return false
}

// rfc822Zones is RFC 822's named zones as RFC 3339 offsets, in lower case.
var rfc822Zones = map[string]string{
	"ut": "Z", "gmt": "Z", "z": "Z",
	"est": "-05:00", "edt": "-04:00", "cst": "-06:00", "cdt": "-05:00",
	"mst": "-07:00", "mdt": "-06:00", "pst": "-08:00", "pdt": "-07:00",
}

// asciiLower is s with its ASCII letters in lower case, as RFC 822's names
// may be in any case.
func asciiLower(s string) string {
	b := []byte(s)
	for i, c := range b {
		if 'A' <= c && c <= 'Z' {
			b[i] = c + ('a' - 'A')
		}
	}
	return string(b)
}

// rfc822 is an RSS date, RFC 822's date-time with a four-digit year
// allowed and its names in any case, as the same instant in RFC 3339's
// form, as the feed render writes one: a day of the week and a comma at
// most, then the day, the month, the year (two digits before 50 in the
// 2000s, else in the 1900s), the time (seconds 00 where it has none) and
// the zone (Z for UT, GMT and Z, the US zones' offsets, or a sign and four
// digits). Its tabs and line breaks are spaces.
func rfc822(s string) (string, bool) {
	words := func(t string) []string {
		var out []string
		for _, w := range strings.Split(strings.NewReplacer("\n", " ", "\r", " ", "\t", " ").Replace(t), " ") {
			if w != "" {
				out = append(out, w)
			}
		}
		return out
	}
	var w []string
	switch parts := strings.Split(s, ","); len(parts) {
	case 1:
		w = words(s)
	case 2:
		d := words(parts[0])
		days := map[string]bool{"mon": true, "tue": true, "wed": true, "thu": true, "fri": true, "sat": true, "sun": true}
		if len(d) != 1 || !days[asciiLower(d[0])] {
			return "", false
		}
		w = words(parts[1])
	default:
		return "", false
	}
	if len(w) != 5 {
		return "", false
	}
	day, month, year, clock, zone := w[0], w[1], w[2], w[3], w[4]
	switch {
	case digitsOf(4, year):
	case digitsOf(2, year) && year < "50":
		year = "20" + year
	case digitsOf(2, year):
		year = "19" + year
	default:
		return "", false
	}
	months := []string{"jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"}
	m := -1
	for i, name := range months {
		if name == asciiLower(month) {
			m = i
		}
	}
	if m < 0 {
		return "", false
	}
	if digitsOf(1, day) {
		day = "0" + day
	}
	if !twoDigits(1, 31, day) {
		return "", false
	}
	hms := strings.Split(clock, ":")
	if len(hms) == 2 {
		hms = append(hms, "00")
	}
	if !(len(hms) == 3 && twoDigits(0, 23, hms[0]) && twoDigits(0, 59, hms[1]) && twoDigits(0, 60, hms[2])) {
		return "", false
	}
	numeric := func(sign, digits string) (string, bool) {
		if digitsOf(4, digits) && twoDigits(0, 23, digits[:2]) && twoDigits(0, 59, digits[2:]) {
			return sign + digits[:2] + ":" + digits[2:], true
		}
		return "", false
	}
	// A sign and four digits; the render splits the zone at its signs, so
	// one with a second sign is no zone.
	offset, ok := rfc822Zones[asciiLower(zone)]
	if !ok {
		if plus := strings.Split(zone, "+"); len(plus) == 2 && plus[0] == "" {
			offset, ok = numeric("+", plus[1])
		} else if minus := strings.Split(zone, "-"); len(minus) == 2 && minus[0] == "" {
			offset, ok = numeric("-", minus[1])
		}
	}
	if !ok {
		return "", false
	}
	return fmt.Sprintf("%s-%02d-%sT%s%s", year, m+1, day, strings.Join(hms, ":"), offset), true
}

// feedDate is a date as the feed render writes it: its RFC 3339 form, or,
// with ok false, the text of one in neither form, which it writes as the
// epoch with a category that keeps the text. A date with no text is a
// missing one.
func feedDate(v *tt.Datum) (string, bool) {
	if v.Kind != tt.DatumString {
		return v.String(), false
	}
	if v.Text == "" {
		return feedEpoch, true
	}
	upper := strings.NewReplacer("t", "T", "z", "Z").Replace(v.Text)
	if rfc3339(upper) {
		return upper, true
	}
	if c, ok := rfc822(v.Text); ok {
		return c, true
	}
	return v.Text, false
}

// feedClean is a feed's value as the reader builds it back: no member
// whose value is null, and a character XML 1.0 cannot carry as U+FFFD.
func feedClean(d tt.Datum) tt.Datum {
	switch d.Kind {
	case tt.DatumString:
		return tt.StringDatum(strings.Map(func(c rune) rune {
			if (c < 0x20 && c != '\t' && c != '\n' && c != '\r') || c == 0xFFFE || c == 0xFFFF {
				return 0xFFFD
			}
			return c
		}, d.Text))
	case tt.DatumArray:
		items := make([]tt.Datum, len(d.Items))
		for i, item := range d.Items {
			items[i] = feedClean(item)
		}
		return tt.ArrayDatum(items...)
	case tt.DatumObject:
		members := []tt.Member{}
		for _, m := range d.Members {
			if m.Value.Kind != tt.DatumNull {
				members = append(members, tt.Member{Key: m.Key, Value: feedClean(m.Value)})
			}
		}
		return tt.ObjectDatum(members...)
	}
	return d
}

// feedText is a text construct or a content as the feed render writes it:
// one of type xhtml as html, its value trimmed.
func feedText(d tt.Datum) tt.Datum {
	cleaned := feedClean(d)
	if cleaned.Kind != tt.DatumObject {
		return cleaned
	}
	if t, ok := cleaned.Get("type"); !ok || t.Kind != tt.DatumString || t.Text != "xhtml" {
		return cleaned
	}
	members := make([]tt.Member, len(cleaned.Members))
	for i, m := range cleaned.Members {
		switch {
		case m.Key == "type":
			m.Value = tt.StringDatum("html")
		case m.Key == "value" && m.Value.Kind == tt.DatumString:
			m.Value = tt.StringDatum(strings.TrimSpace(m.Value.Text))
		}
		members[i] = m
	}
	return tt.ObjectDatum(members...)
}

// feedHasAuthor is whether a feed or an entry has an author: a list of them
// that is not empty.
func feedHasAuthor(d *tt.Datum) bool {
	if d.Kind != tt.DatumObject {
		return false
	}
	authors, ok := d.Get("authors")
	return ok && authors.Kind == tt.DatumArray && len(authors.Items) > 0
}

// feedObject is a feed's or an entry's members as the feed render writes
// them and the reader builds them back (feedReading): each date in RFC
// 3339's form, one the render cannot read as the epoch with a category
// keeping its text, which comes before the object's own categories where
// the date comes before them in the tree's order; text constructs as
// feedText; an id, a title and an updated where the object has none, an
// entry's id with a slash and position (negative for the feed); an entry's
// source not read back.
func feedObject(d tt.Datum, position int) []tt.Member {
	var out []tt.Member
	var before, after []tt.Datum
	categoriesMet := false
	for _, m := range d.Members {
		if m.Value.Kind == tt.DatumNull {
			continue
		}
		switch m.Key {
		case "categories":
			categoriesMet = true
		case "source", "format", "version", "entries":
			continue
		}
		var value tt.Datum
		switch m.Key {
		case "updated", "published":
			date, ok := feedDate(&m.Value)
			if ok {
				value = tt.StringDatum(date)
				break
			}
			category := tt.ObjectDatum(
				tt.Member{Key: "term", Value: tt.StringDatum(m.Key)},
				tt.Member{Key: "scheme", Value: tt.StringDatum(feedRender + "/date")},
				tt.Member{Key: "label", Value: feedClean(tt.StringDatum(date))},
			)
			if categoriesMet {
				after = append(after, category)
			} else {
				before = append(before, category)
			}
			value = tt.StringDatum(feedEpoch)
		case "title", "subtitle", "rights", "summary", "content":
			value = feedText(m.Value)
		default:
			value = feedClean(m.Value)
		}
		out = append(out, tt.Member{Key: m.Key, Value: value})
	}
	if len(before) > 0 || len(after) > 0 {
		var own []tt.Datum
		for i, m := range out {
			if m.Key == "categories" {
				own = m.Value.Items
				out = append(out[:i], out[i+1:]...)
				break
			}
		}
		all := append(append(before, own...), after...)
		out = append(out, tt.Member{Key: "categories", Value: tt.ArrayDatum(all...)})
	}
	has := func(key string) bool {
		for _, m := range out {
			if m.Key == key {
				return true
			}
		}
		return false
	}
	if !has("id") {
		id := feedRender
		if position >= 0 {
			id = feedRender + "/" + strconv.Itoa(position)
		}
		out = append(out, tt.Member{Key: "id", Value: tt.StringDatum(id)})
	}
	if !has("title") {
		out = append(out, tt.Member{Key: "title", Value: tt.ObjectDatum(
			tt.Member{Key: "type", Value: tt.StringDatum("text")},
			tt.Member{Key: "value", Value: tt.StringDatum("")},
		)})
	}
	if !has("updated") {
		out = append(out, tt.Member{Key: "updated", Value: tt.StringDatum(feedEpoch)})
	}
	return out
}

// feedReading is what a feed reads back as, by its loss list: an Atom 1.0
// feed of its members and its entries as feedObject writes them, and an
// author, named by the feed's title where that is text and not empty and
// unknown otherwise, where the feed has none, though every entry may have
// one: the render writes a feed's elements before its entries, each entry
// as it comes, so it cannot see them first (feed v0.6.19). Go's feed
// reader is a registered defect, so no feed is a source here until it is
// repaired.
func feedReading(d tt.Datum) tt.Datum {
	var entries []tt.Datum
	given, hasEntries := d.Get("entries")
	if hasEntries && given.Kind == tt.DatumArray {
		for i, e := range given.Items {
			entries = append(entries, tt.ObjectDatum(feedObject(e, i)...))
		}
	}
	out := []tt.Member{
		{Key: "format", Value: tt.StringDatum("atom")},
		{Key: "version", Value: tt.StringDatum("1.0")},
	}
	if hasEntries {
		out = append(out, tt.Member{Key: "entries", Value: tt.ArrayDatum(entries...)})
	}
	out = append(out, feedObject(d, -1)...)
	if !feedHasAuthor(&d) {
		name := tt.StringDatum("unknown")
		if title, ok := d.Get("title"); ok && title.Kind == tt.DatumObject {
			typ, _ := title.Get("type")
			value, _ := title.Get("value")
			if typ != nil && typ.Kind == tt.DatumString && typ.Text == "text" &&
				value != nil && value.Kind == tt.DatumString && value.Text != "" {
				name = *value
			}
		}
		kept := out[:0]
		for _, m := range out {
			if m.Key != "authors" {
				kept = append(kept, m)
			}
		}
		out = append(kept, tt.Member{Key: "authors", Value: tt.ArrayDatum(tt.ObjectDatum(
			tt.Member{Key: "name", Value: name},
			tt.Member{Key: "uri", Value: tt.StringDatum(feedRender)},
		))})
	}
	return tt.ObjectDatum(out...)
}

// check is whether the document read back from written in target is what
// the target's conventions make of source, read as from.
// checkGrammar is whether a grammar written in a grammar notation reads
// back as the notations' conventions say. A render writes a grammar's rules
// anew, as far as its notation can say them, and its contract is the round
// trip (each render's header): a tree of the target's own notation reads
// back as it was; one of another notation, written as the target spells
// what the source spells otherwise (its loss list), reads back as a tree
// the render writes again as the same text.
func checkGrammar(from, target *translate.Format, source tt.Datum, written string) error {
	back, f := target.Read(written, tt.DefaultLimits())
	if f != nil {
		return fmt.Errorf("the written grammar does not read back: %v", f)
	}
	if from.ID() == target.ID() {
		if same(&source, &back) {
			return nil
		}
		return fmt.Errorf("read back as %s, where %s was written", back.String(), source.String())
	}
	again, f := translateText(target, target, written, nil)
	if f != nil {
		return fmt.Errorf("the grammar it reads back as is not written again: %v; it was written as %q", f, written)
	}
	if again == written {
		return nil
	}
	return fmt.Errorf("reads back as a grammar that is written again as %q, where %q was written", again, written)
}

// compiledSpec is the grammar spec a grammar notation's document compiles
// to, with its own notation's compiler, as that module writes it for a
// host that installs a grammar: the pure-data spec, recognition off, so
// that the tree builders stay, as strict JSON, read as JSON (AbnfCompile
// for ABNF, and for EBNF and GBNF the shared compiler's serializers over
// the conversion with the builtins on, which abnf's module aliases). The
// compiler's refusal is the error: the reader reads a grammar as its
// rules whether or not they compile.
func compiledSpec(t *testing.T, id, text string) (tt.Datum, error) {
	pure := func(spec *tabnas.GrammarSpec, err error) (string, error) {
		if err != nil {
			return "", err
		}
		p, err := tabnasabnf.ToPureSpec(spec)
		if err != nil {
			return "", err
		}
		return tabnasabnf.ToJsonic(p, true, 0), nil
	}
	var json string
	var err error
	switch id {
	case "abnf":
		recognition := false
		json, err = tabnasabnf.AbnfCompile(text, &tabnasabnf.AbnfCompileOptions{Recognition: &recognition, Strict: true})
	case "ebnf":
		json, err = pure(tabnasebnf.Ebnf(text, &tabnasebnf.ConvertOptions{Builtins: true}))
	case "gbnf":
		json, err = pure(tabnasgbnf.Gbnf(text, &tabnasgbnf.ConvertOptions{Builtins: true}))
	default:
		t.Fatalf("%s is no grammar notation", id)
	}
	if err != nil {
		return tt.Datum{}, err
	}
	spec, f := format(t, "json").Read(json, tt.DefaultLimits())
	if f != nil {
		return tt.Datum{}, f
	}
	return spec, nil
}

// exactLexing is whether a grammar spec sets the lexing GBNF's compiler
// gives a spec: exact, no white space skipped and no matcher of the
// engine's own (space.lex off).
func exactLexing(spec *tt.Datum) bool {
	options, ok := spec.Get("options")
	if !ok {
		return false
	}
	space, ok := options.Get("space")
	if !ok {
		return false
	}
	lex, ok := space.Get("lex")
	return ok && lex.Kind == tt.DatumBool && !lex.Bool
}

// grammarEngine is the engine with a grammar spec installed, a fresh
// instance each: installing applies the spec's lexer options to the
// instance.
func grammarEngine(spec *tt.Datum) (*tabnas.Tabnas, error) {
	gs, err := tabnas.GrammarSpecFromJSON([]byte(spec.String()))
	if err != nil {
		return nil, err
	}
	engine := tabnas.Make()
	if err := engine.Grammar(gs); err != nil {
		return nil, err
	}
	return engine, nil
}

// recognition is what from's document, text, written in target as written,
// recognises against what it recognised, over the document's samples: each
// grammar is compiled by its own notation's compiler (compiledSpec) and
// installed on an engine of its own, each sample is parsed with both, and
// both accept it or both refuse it. A render writes a grammar as far as its
// notation can say it and recognises what it recognised, but for the
// lexing: a grammar of another notation compiles under the target's own
// settings (its loss list), so across GBNF's exact lexing and the others'
// default one, which skips white space, a sample holding white space may be
// recognised otherwise, as declared; the caller holds those to the samples
// test/notation-samples.json registers for the pair. It is the number of
// samples compared and the samples recognised otherwise across the lexing;
// uncompiled, where the document's own compiler refuses it, why, since the
// reader reads a grammar as its rules whether or not they compile; and the
// error, a written grammar its own compiler refuses among them.
func recognition(t *testing.T, from, target *translate.Format, text, written string, samples []string) (int, []string, string, error) {
	if len(samples) == 0 {
		return 0, nil, "", nil
	}
	source, err := compiledSpec(t, from.ID(), text)
	if err != nil {
		return 0, nil, err.Error(), nil
	}
	back, err := compiledSpec(t, target.ID(), written)
	if err != nil {
		return 0, nil, "", fmt.Errorf("the written grammar does not compile: %v", err)
	}
	across := exactLexing(&source) != exactLexing(&back)
	read, err := grammarEngine(&source)
	if err != nil {
		return 0, nil, "", fmt.Errorf("the spec read does not install: %v", err)
	}
	writtenEngine, err := grammarEngine(&back)
	if err != nil {
		return 0, nil, "", fmt.Errorf("the spec written does not install: %v", err)
	}
	var otherwise []string
	verb := map[bool]string{true: "accepts", false: "refuses"}
	for _, sample := range samples {
		_, wasErr := read.Parse(sample)
		_, isErr := writtenEngine.Parse(sample)
		was, is := wasErr == nil, isErr == nil
		if was == is {
			continue
		}
		if across && strings.ContainsAny(sample, " \t\n\r") {
			otherwise = append(otherwise, sample)
			continue
		}
		return 0, nil, "", fmt.Errorf("recognises %q otherwise: the grammar read %s it, the one written %s it, where %q "+
			"was written", sample, verb[was], verb[is], written)
	}
	return len(samples), otherwise, "", nil
}

func check(t testing.TB, from, target *translate.Format, source tt.Datum, written string) error {
	if grammarNotation(target) {
		return checkGrammar(from, target, source, written)
	}
	limits := tt.DefaultLimits()
	id := target.ID()
	// A source whose events carry the target's own schema (XML's element
	// tree into XML) is written as it is, and reads back as it is.
	embedded := target.Part.Schema != "" && from.Part.Schema != target.Part.Schema
	var back tt.Datum
	switch {
	case id == "markdown":
		// A Markdown table reads back as records through the format's
		// lift, as a host reads it for a records target.
		lift := target.Part.Lift
		if lift == nil {
			t.Fatal("markdown has a lift")
		}
		program := lift.Text + "\ndef export [input] (records (" + lift.Entry + " input))\n"
		yaml, f := translateText(target, format(t, "yaml"), written, &alchemy.Source{File: "markdown-records.alc", Text: program})
		if f != nil {
			return fmt.Errorf("the written table does not read back: %v", f)
		}
		if back, f = format(t, "yaml").Read(yaml, limits); f != nil {
			return fmt.Errorf("its records do not read back: %v", f)
		}
	case (id == "xml" || id == "feed") && embedded:
		// An embedding reads back through its reverse, which its file holds
		// beside it (xml-unembed, feed-unembed): the format's tree, as JSON,
		// unembedded, and written where a non-finite number has a spelling.
		tree, f := target.Read(written, limits)
		if f != nil {
			return fmt.Errorf("the written document does not read back: %v", f)
		}
		embed := target.Part.Embed
		if embed == nil || !strings.HasSuffix(embed.Entry, "-embed") {
			t.Fatalf("%s: an embedding target has an embed named NAME-embed", id)
		}
		reverse := strings.TrimSuffix(embed.Entry, "-embed") + "-unembed"
		program := embed.Text + "\ndef export [input] (" + reverse + " input)\n"
		yaml, f := translateText(format(t, "json"), format(t, "yaml"), tree.String(), &alchemy.Source{File: reverse + ".alc", Text: program})
		if f != nil {
			return fmt.Errorf("the tree does not unembed: %v", f)
		}
		if back, f = format(t, "yaml").Read(yaml, limits); f != nil {
			return fmt.Errorf("the unembedded tree does not read back: %v", f)
		}
	default:
		var f *tt.Fail
		if back, f = target.Read(written, limits); f != nil {
			return fmt.Errorf("the written document does not read back: %v", f)
		}
	}
	key := at.DefaultOptions().Key
	null := func(float64) tt.Datum { return tt.NullDatum() }
	var expected tt.Datum
	switch id {
	case "csv":
		return checkRecords(source, back, func(t string) string { return t })
	case "markdown":
		return checkRecords(source, back, markdownCell)
	case "ini":
		expected := wrapObject(source, key)
		if iniSame(&expected, &back) {
			return nil
		}
		return fmt.Errorf("read back as %s, where %s was written", back, expected)
	case "json", "jsonc", "jsonic":
		expected = mapNonFinite(source, null)
	case "jsonl":
		expected = mapNonFinite(wrapArray(source), null)
	case "toml":
		expected = withoutNulls(wrapObject(source, key))
	case "zon":
		expected = zonReading(source)
		back = zonBack(back)
	case "expr":
		expected = exprReading(source)
	case "semver":
		expected = semverReading(source)
	case "feed":
		// A feed's own tree, written as Atom 1.0, with no member whose value
		// is null, so a null member read back is one left out (an embedded
		// tree reads back through its reverse, above, as it was).
		if embedded {
			expected = source
		} else {
			expected = feedReading(source)
			back = feedClean(back)
		}
	default:
		expected = source
	}
	if same(&expected, &back) {
		return nil
	}
	return fmt.Errorf("read back as %s, where %s was written", back, expected)
}

// depth is how deep a value nests: a scalar is 0, a container one more
// than its deepest member.
func depth(d *tt.Datum) int {
	deepest := 0
	switch d.Kind {
	case tt.DatumArray:
		for i := range d.Items {
			deepest = max(deepest, depth(&d.Items[i]))
		}
	case tt.DatumObject:
		for i := range d.Members {
			deepest = max(deepest, depth(&d.Members[i].Value))
		}
	default:
		return 0
	}
	return 1 + deepest
}

// depthBound is the nesting every format reads: the readers guard nesting
// at different depths (tabnas-json past 128 levels, YAML's and ZON's near
// it, the transducer at 256 events deep, which XML's embedding reaches at
// about 127 levels, two elements a level), and a root adapter or an
// embedding adds a level or two. A document nested deeper than this is at
// one format's guard and past another's, a limit and not a shape, so the
// matrix leaves it out, and pins how few such documents there are.
const depthBound = 100

// size is how many values a tree holds: a scalar is one, a container one
// more than its members hold.
func size(d *tt.Datum) int {
	n := 1
	for i := range d.Items {
		n += size(&d.Items[i])
	}
	for i := range d.Members {
		n += size(&d.Members[i].Value)
	}
	return n
}

// sizeBound is the size of a tree every format writes in moments. A tree
// of half a million values (the grammar spec RFC 3986's URI grammar
// compiles to holds 513,409) takes seconds into JSON and minutes into an
// XML embedding, and the TypeScript command's interpreter takes minutes
// over 14,000 (the grammar specs the larger GBNF examples compile to, C's
// and JSON's): a workload and not a shape, so the matrix leaves a document
// past this out, as the TypeScript one does, and pins how many there are.
// None is, since the grammar notations are read as their rules (the Rust
// suite's release run, faster, reads trees ten times this size).
const sizeBound = 10_000

// bounds is what a run of the matrix is held to: at least floor
// documents, at most tooDeep of them deeper than every format reads and
// tooLarge larger than every format writes in moments, at least refusals
// pairs refused as their target declares, at least grammarsWritten
// grammars written in a grammar notation, at least samplesCompared samples
// of theirs (notation's Samples, by document) compared between the grammar
// read and the one written, each recognised by both alike but those
// notation's Otherwise registers for the pair, and exactly uncompiled pairs
// whose document has samples and is one its own notation's compiler
// refuses, so that nothing is compared.
type bounds struct {
	floor, tooDeep, tooLarge, refusals, grammarsWritten, samplesCompared, uncompiled int
	// whole is the documents a source's part refuses part way, at most:
	// each is read with --whole, and its pairs are held to what that
	// writes.
	whole    int
	notation notation
}

// unstreamed is whether a translation failed because the source's
// transducer part cannot stream the document (a repeated member, a stream
// of several YAML documents, two plain << in one YAML mapping, a YAML merge
// source that repeats a merged key): its code, and the route that reads it
// named, since the output streams and is never written twice
// (admin ADR-33).
func unstreamed(f *tt.Fail) bool {
	return f != nil && (f.Code == tt.CodeStreamabilityUnknown || f.Code == tt.CodeDuplicateMember) &&
		strings.HasSuffix(f.Message, "; --whole reads the document whole")
}

// divergent is the pairs that fail in this runtime, and not in Rust's, for
// a defect outside this repository: by the name the matrix gives a pair,
// each with its defect. The matrix holds each to failing, so an entry
// cannot outlive the defect it records: one that passes, or that names no
// pair of the corpus, fails the test until it is deleted. There is none.
var divergent = map[string]string{}

// matrix is the cross product of docs and every format: each document read
// with its format's grammar, written in every format, and read back under
// the target's conventions, or refused as the target declares, held to b;
// a document of a format whose reader is a registered defect is left out
// as a source, and counted.
func matrix(t *testing.T, docs []document, b bounds) {
	limits := tt.DefaultLimits()
	targets := translate.Formats()
	if len(targets) != 22 {
		t.Fatalf("the formats: %s", translate.Names())
	}
	total := len(docs) * len(targets)
	var failures, refusedSources, tooDeep, tooLarge, defective, refusals, diverged, repaired []string
	met := map[string]bool{}
	pairs, grammarsWritten, samplesCompared := 0, 0, 0
	var declaredOtherwise, uncompiled []string
	readWhole := map[string]string{}
	var readWholeOrder []string
	started := time.Now()
	reported := time.Now()
	for n, doc := range docs {
		from := format(t, doc.format)
		var source tt.Datum
		read := false
		if registeredDefect(from.ID()) {
			defective = append(defective, doc.name)
		} else if value, f := from.Read(doc.text, limits); f != nil {
			refusedSources = append(refusedSources, fmt.Sprintf("%s: %v", doc.name, f))
		} else if d := depth(&value); d > depthBound {
			tooDeep = append(tooDeep, fmt.Sprintf("%s: %d levels", doc.name, d))
		} else if n := size(&value); n > sizeBound {
			tooLarge = append(tooLarge, fmt.Sprintf("%s: %d values", doc.name, n))
		} else {
			source, read = value, true
		}
		for _, to := range targets {
			if !read {
				break
			}
			pairs++
			name := fmt.Sprintf("%s (%s) -> %s", doc.name, from.ID(), to.ID())
			held := expect(from, to, &source)
			written, f := translateText(from, to, doc.text, nil)
			// A document the source's part cannot stream fails, naming
			// --whole; read whole, the pair is held to what that writes, and
			// the document is counted.
			if unstreamed(f) {
				key := fmt.Sprintf("%s (%s)", doc.name, from.ID())
				if _, seen := readWhole[key]; !seen {
					readWhole[key] = f.Code.String()
					readWholeOrder = append(readWholeOrder, key)
				}
				written, f = translateAs(from, to, doc.text, nil, true)
			}
			var why error
			switch {
			case !held.written && f == nil:
				why = fmt.Errorf("is written, where it is declared refused (%s, %s...): %q", held.code, held.reason, written)
			case !held.written && (f.Code != held.code || !strings.HasPrefix(f.Message, held.reason)):
				why = fmt.Errorf("is refused otherwise than declared (%s, %s...): %v", held.code, held.reason, f)
			case !held.written:
				refusals = append(refusals, name+": "+held.reason)
			case f != nil && held.unless && f.Code == held.code && strings.HasPrefix(f.Message, held.reason):
				refusals = append(refusals, name+": "+f.Message)
			case f != nil:
				why = fmt.Errorf("does not write: %v", f)
			// A records source read through its lift writes its table, not
			// its tree: compare with the table.
			case from.Part.Reads[0] == at.ShapeRecords && to.Part.Writes == at.ShapeRecords:
				if _, f := to.Read(written, limits); f != nil {
					why = fmt.Errorf("the written document does not read back: %v", f)
				}
			default:
				if held.unless {
					grammarsWritten++
				}
				why = check(t, from, to, source, written)
				if why == nil && held.unless {
					compared, otherwise, refused, err := recognition(t, from, to, doc.text, written, b.notation.Samples[doc.name])
					why = err
					samplesCompared += compared
					registered := b.notation.Otherwise[name]
					switch {
					case err != nil:
					case refused != "":
						uncompiled = append(uncompiled, name+": "+refused)
					case !slices.Equal(otherwise, registered):
						why = fmt.Errorf("recognises %s otherwise across the lexing, where test/notation-samples.json "+
							"registers %s for the pair", jsonList(otherwise), jsonList(registered))
					case len(otherwise) > 0:
						declaredOtherwise = append(declaredOtherwise, fmt.Sprintf("%s: %d of %d samples", name,
							len(otherwise), compared))
					}
				}
			}
			_, known := divergent[name]
			if known {
				met[name] = true
			}
			switch {
			case why != nil && known:
				diverged = append(diverged, fmt.Sprintf("%s: %v", name, why))
			case why != nil:
				failures = append(failures, fmt.Sprintf("%s: %v", name, why))
			case known:
				repaired = append(repaired, name)
			}
		}
		if (n+1)%25 == 0 || n+1 == len(docs) || time.Since(reported) > 20*time.Second {
			reported = time.Now()
			t.Logf("matrix: %d of %d documents (%d%%), %d failures, %ds", n+1, len(docs), (n+1)*100/len(docs), len(failures),
				int(time.Since(started).Seconds()))
		}
	}
	for _, line := range refusedSources {
		t.Logf("refused source: %s", line)
	}
	for _, line := range tooDeep {
		t.Logf("deeper than every format reads: %s", line)
	}
	for _, line := range tooLarge {
		t.Logf("larger than %d values: %s", sizeBound, line)
	}
	for _, line := range declaredOtherwise {
		t.Logf("recognised otherwise across the lexing, as the loss lists declare and test/notation-samples.json "+
			"registers: %s holding white space", line)
	}
	for _, line := range diverged {
		t.Logf("divergent, as registered: %s", line)
	}
	for _, key := range readWholeOrder {
		t.Logf("read with --whole, which its part cannot stream: %s: %s", key, readWhole[key])
	}
	for _, line := range uncompiled {
		t.Logf("no recognition to compare, its own compiler refusing the document: %s", line)
	}
	for _, line := range failures {
		t.Logf("FAIL %s", line)
	}
	schemaOnly, unwritable, exprRefused := 0, 0, 0
	for _, r := range refusals {
		switch {
		case strings.Contains(r, "schema_only:"):
			schemaOnly++
		case strings.Contains(r, ": the grammar cannot be written as "):
			unwritable++
		case strings.Contains(r, ": the object at "):
			exprRefused++
		}
	}
	t.Logf("matrix: %d pairs of %d documents; %d refused by their own reader, %d too deep, %d too large, %d left out "+
		"for a registered reader defect, %d divergent as registered, %d read with --whole, which their parts cannot "+
		"stream; %d pairs refused as their target declares (%d by a "+
		"schema-only target, %d by a grammar notation's render, %d by expr's render, %d by Semantic Versioning's "+
		"embedding); %d grammars written in a grammar notation, %d of their samples compared, %d pairs recognising "+
		"some otherwise across the lexing, as registered, %d pairs with samples whose document its own compiler "+
		"refuses",
		pairs, len(docs), len(refusedSources), len(tooDeep), len(tooLarge), len(defective), len(diverged), len(readWhole),
		len(refusals),
		schemaOnly, unwritable, exprRefused, len(refusals)-schemaOnly-unwritable-exprRefused, grammarsWritten,
		samplesCompared, len(declaredOtherwise), len(uncompiled))
	leftOut := len(refusedSources) + len(tooDeep) + len(tooLarge) + len(defective)
	if pairs+leftOut*len(targets) != total || len(docs) < b.floor {
		t.Fatalf("the corpora shrank: %d documents", len(docs))
	}
	if len(tooDeep) > b.tooDeep {
		t.Fatalf("%d documents are deeper than every format reads (above)", len(tooDeep))
	}
	if len(readWhole) > b.whole {
		t.Fatalf("%d documents are read with --whole, more than the %d the corpus gives (above)", len(readWhole), b.whole)
	}
	if len(tooLarge) > b.tooLarge {
		t.Fatalf("%d documents hold more than %d values (above)", len(tooLarge), sizeBound)
	}
	if len(refusals) < b.refusals {
		t.Fatalf("%d pairs are refused as their target declares, fewer than the %d the corpora give", len(refusals), b.refusals)
	}
	if len(uncompiled) != b.uncompiled {
		t.Fatalf("%d pairs with samples have a document its own compiler refuses (above)", len(uncompiled))
	}
	if samplesCompared < b.samplesCompared {
		t.Fatalf("%d samples are compared, fewer than the %d the corpora give", samplesCompared, b.samplesCompared)
	}
	if grammarsWritten < b.grammarsWritten {
		t.Fatalf("%d grammars are written in a grammar notation, fewer than the %d the corpora give", grammarsWritten,
			b.grammarsWritten)
	}
	for name := range divergent {
		if !met[name] {
			t.Errorf("the registered divergent pair %q is no pair of the corpus: delete its entry", name)
		}
	}
	for _, name := range repaired {
		t.Errorf("the registered divergent pair %q translates as the conventions say: delete its entry", name)
	}
	if len(failures) > 0 {
		t.Fatalf("%d of %d pairs failed (above)", len(failures), pairs)
	}
}

// A path is a JSON array of keys and whole numbers: an index is the
// number's value, however it is spelled, up to 2^64, where it saturates
// as Rust's float-to-integer cast does; anything else is refused with the
// text as given.
func TestParsePathReadsKeysAndIndexes(t *testing.T) {
	limits := tt.DefaultLimits()
	for _, c := range []struct{ text, want string }{
		{`[]`, `[]`},
		{`["people",0,"name"]`, `["people",0,"name"]`},
		{`[1e2,-0,0.0]`, `[100,0,0]`},
		{`[9007199254740993]`, `[9007199254740992]`},
		{`[1e19]`, `[10000000000000000000]`},
		{`[18446744073709551616]`, `[18446744073709551615]`},
	} {
		path, f := translate.ParsePath(c.text, limits)
		if f != nil || path.String() != c.want {
			t.Errorf("%s: %v %v", c.text, path, f)
		}
	}
	for _, text := range []string{`{}`, `"a"`, `[1.5]`, `[-1]`, `[true]`, `[null]`, `[[0]]`, `[1e300]`, ``, `[1`} {
		path, f := translate.ParsePath(text, limits)
		if f == nil || f.Code != tt.CodeInputInvalid ||
			f.Message != `--path takes a JSON array of keys and indexes, such as ["people",0], not `+text {
			t.Errorf("%q: %v %v", text, path, f)
		}
	}
}

// TestEveryDocumentTranslatesIntoEveryFormat is transduce's fixtures, one
// document per format at least, the documents of JSONTestSuite every JSON
// parser must accept, and the grammar notations' example grammars, into
// every format.
func TestEveryDocumentTranslatesIntoEveryFormat(t *testing.T) {
	matrix(t, corpus(t), bounds{floor: 151, tooDeep: 0, tooLarge: 0, whole: 11, refusals: 1107, grammarsWritten: 45,
		samplesCompared: 364, uncompiled: 0, notation: notationSamples(t)})
}

// request is a request from from to to, with no path and no program.
func request(from, to *translate.Format) *translate.Request {
	return &translate.Request{From: from, To: to, Options: at.DefaultOptions(), Limits: tt.DefaultLimits()}
}

// ZON's reader builds an integer no float64 holds exactly as a *big.Int,
// which this command reads as the object {"$big": "<digits>"}, the digits
// after a minus sign when it is negative, as ZON's part declares and its
// Rust reader builds; ZON's render writes it back as the integer.
func TestAZonIntegerNoFloatHoldsIsTheBigObject(t *testing.T) {
	zon, json := format(t, "zon"), format(t, "json")
	text := ".{ 1, 12345678901234567890, -12345678901234567890, 0xc1ce108124179e16 }\n"
	want := `[1,{"$big":"12345678901234567890"},{"$big":"-12345678901234567890"},{"$big":"13965117641364839958"}]`
	got, f := translateText(zon, json, text, nil)
	if f != nil {
		t.Fatal(f)
	}
	if strings.TrimRight(got, "\n") != want {
		t.Fatalf("got %s, want %s", got, want)
	}
	back, f := translateText(zon, zon, text, nil)
	if f != nil {
		t.Fatal(f)
	}
	if got, f = translateText(zon, json, back, nil); f != nil || strings.TrimRight(got, "\n") != want {
		t.Fatalf("read back from %q: got %s (%v), want %s", back, got, f, want)
	}
}

// The metrics a run is given count what it wrote, whether it streamed
// through the transducer part or read the document whole (--whole), and a
// run the part gives up writes nothing twice: it fails, its metrics what it
// had written.
func TestARunsMetricsAreItsOwn(t *testing.T) {
	json := format(t, "json")
	for _, c := range []struct {
		text  string
		whole bool
	}{{`{"a": [1, 2]}`, false}, {`{"a": 1, "a": 2}`, true}} {
		var out bytes.Buffer
		metrics := tt.NewMetrics()
		r := request(json, json)
		r.Whole = c.whole
		if f := translate.Run(r, c.text, &out, metrics); f != nil {
			t.Fatalf("%s: %v", c.text, f)
		}
		if out.Len() == 0 || metrics.OutputBytes.Load() != uint64(out.Len()) || metrics.Events.Load() == 0 {
			t.Errorf("%s: wrote %d bytes; the metrics count %d output bytes and %d events",
				c.text, out.Len(), metrics.OutputBytes.Load(), metrics.Events.Load())
		}
	}
	var out bytes.Buffer
	metrics := tt.NewMetrics()
	if f := translate.Run(request(json, json), `{"a": 1, "a": 2}`, &out, metrics); f == nil || f.Code != tt.CodeDuplicateMember {
		t.Errorf("a repeated member, streamed: %v", f)
	}
	if metrics.OutputBytes.Load() != uint64(out.Len()) {
		t.Errorf("the metrics count %d output bytes, the writer took %d", metrics.OutputBytes.Load(), out.Len())
	}
}

// takes is a writer that takes n bytes, then fails.
type takes struct{ n int }

func (w *takes) Write(p []byte) (int, error) {
	n := min(w.n, len(p))
	w.n -= n
	if n < len(p) {
		return n, errors.New("the disk is full")
	}
	return n, nil
}

// A writer that fails once it has taken some of the output: the failure
// says the output had left, and says it had not when it took none.
func TestAnOutputFailureAfterBytesLeftIsCommitted(t *testing.T) {
	json := format(t, "json")
	for _, c := range []struct {
		takes     int
		committed bool
	}{{0, false}, {3, true}} {
		f := translate.Run(request(json, json), "[1, 2, 3]", &takes{c.takes}, tt.NewMetrics())
		if f == nil || f.Code != tt.CodeOutputFailed || f.CommittedOutput != c.committed {
			t.Errorf("taking %d bytes: %v", c.takes, f)
		}
	}
}
