// Copyright (c) 2026 tabnas, MIT License

package translate

// transducer_test.go: a format whose module ships a transducer part is read
// through it (the Rust crate's unit tests in rs/src/translate.rs): the
// route over its parser's rule passes, composed over the part, and what it
// falls back to.

import (
	"bytes"
	"strings"
	"testing"

	alchemy "github.com/tabnas/alchemy/go"
	at "github.com/tabnas/alchemy/go/translate"
	tt "github.com/tabnas/transduce/go"
)

// markingTransduce writes {"ran":true} whatever the document: what it
// writes says which route ran.
const markingTransduce = "def json-transduce [input]\n  as-events\n    scan-emit\n      []\n      fn [s pass] (transition s [])\n      fn [s] [object-start (key \"ran\") (scalar true) object-end]\n      parser-events input\n"

// plainJSON is json without its transducer: read as a format whose module
// ships none is read.
func plainJSON(t *testing.T) *Format {
	t.Helper()
	json := Named("json")
	if json == nil {
		t.Fatal("json is a format")
	}
	return &Format{Part: json.Part, reader: json.reader, passes: json.passes}
}

// transducedJSON is json, read through a transducer part given beside its
// module's parts in place of the one it ships.
func transducedJSON(t *testing.T, text string) *Format {
	t.Helper()
	json := Named("json")
	if json == nil {
		t.Fatal("json is a format")
	}
	return &Format{
		Part:       json.Part,
		reader:     json.reader,
		passes:     json.passes,
		Transducer: &at.Alc{File: "tabnas-json/alchemy/transduce.alc", Entry: "json-transduce", Text: text},
	}
}

func translated(t *testing.T, from *Format, to, path string, program *alchemy.Source, input string) (string, *tt.Fail) {
	t.Helper()
	return translatedAs(t, from, to, path, program, input, false)
}

// translatedAs is translated, read whole (--whole) where whole is asked.
func translatedAs(t *testing.T, from *Format, to, path string, program *alchemy.Source, input string, whole bool) (string, *tt.Fail) {
	t.Helper()
	limits := tt.DefaultLimits()
	target := Named(to)
	if target == nil {
		t.Fatalf("%s is a format", to)
	}
	r := &Request{From: from, To: target, Options: at.DefaultOptions(), Program: program, Whole: whole, Limits: limits}
	if path != "" {
		p, f := ParsePath(path, limits)
		if f != nil {
			t.Fatal(f)
		}
		r.Path = p
	}
	var held bytes.Buffer
	if f := Run(r, input, &held, nil); f != nil {
		return "", f
	}
	return held.String(), nil
}

func mustTranslate(t *testing.T, from *Format, to, path string, program *alchemy.Source, input string) string {
	t.Helper()
	out, f := translated(t, from, to, path, program, input)
	if f != nil {
		t.Fatalf("%s from %q: %v", to, input, f)
	}
	return out
}

// A format whose module ships a transducer part is read through it (json,
// through tabnas/json's), its rule passes streamed into the composition
// over the part, and one without is read whole, its value walked: the two
// routes write the same bytes for the same document, into a tree target, a
// records target, and with a program over the input, but for a number's
// lexeme, which the part keeps and the walk spells as the engine does; a
// marking part given in the module's place shows the route ran. A path
// selects a sub-stream of the same events, so its value keeps its lexemes
// too, and read whole it is selected in the grammar's value. A repeated
// member, which the part streams twice and the composition's tree-contract
// refuses, fails with its code and names --whole, since the output streams
// and is not written twice, under a path as at the root; read whole, it is
// the grammar's value, as a format without a part writes it (admin
// ADR-33).
func TestATransducerPartReadsTheFormatAndTheWalkStandsInWhereItIsAsked(t *testing.T) {
	plain := plainJSON(t)
	through := Named("json")
	marking := transducedJSON(t, markingTransduce)
	if plain.Transducer != nil || through.Transducer == nil || through.Transducer.Entry != "json-transduce" || !strings.Contains(through.Transducer.Text, "transduce-tree") {
		t.Fatalf("plain %v, through %v", plain.Transducer, through.Transducer)
	}
	tree := `{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}`
	// The same document, its numbers spelled as the engine spells them.
	spelled := strings.ReplaceAll(tree, "2.50", "2.5")
	if got := mustTranslate(t, through, "json", "", nil, tree); got != tree+"\n" {
		t.Errorf("the part keeps a number's lexeme: %q", got)
	}
	if got := mustTranslate(t, plain, "json", "", nil, tree); got != spelled+"\n" {
		t.Errorf("the walk spells a number as the engine does: %q", got)
	}
	records := `[{"a":1,"b":"x"},{"a":2,"b":"y"}]`
	echo := &alchemy.Source{File: "echo.alc", Text: "def export [input] (events input)\n"}
	for _, c := range []struct {
		to, doc string
		program *alchemy.Source
	}{
		{"json", spelled, nil},
		{"yaml", spelled, nil},
		{"csv", records, nil},
		{"json", spelled, echo},
		{"csv", records, echo},
	} {
		expected := mustTranslate(t, plain, c.to, "", c.program, c.doc)
		got := mustTranslate(t, through, c.to, "", c.program, c.doc)
		if got != expected {
			t.Errorf("%s from %s: the transducer's route wrote %q, the plain one %q", c.to, c.doc, got, expected)
		}
		if marked, f := translated(t, marking, c.to, "", c.program, c.doc); f == nil && marked == expected {
			t.Errorf("%s from %s: the part's route did not run", c.to, c.doc)
		}
	}
	if got := mustTranslate(t, marking, "json", "", nil, tree); got != "{\"ran\":true}\n" {
		t.Errorf("the marking part wrote %q", got)
	}
	// A path selects a sub-stream of the part's events: its value keeps the
	// lexeme the document spelled a number with, and the marking part's own
	// events are what it selects in. Read whole, or without a part, a path
	// selects in the grammar's value, which spells a number as the engine
	// does; a path that names nothing fails at the document's end.
	for _, c := range []struct {
		from       *Format
		path, want string
		whole      bool
	}{
		{through, `["a",1]`, "2.50\n", false},
		{through, `["a"]`, "[1,2.50,\"x\",{\"b\":null}]\n", false},
		{through, `["a",3,"b"]`, "null\n", false},
		{through, `[]`, tree + "\n", false},
		{marking, `["ran"]`, "true\n", false},
		{through, `["a",1]`, "2.5\n", true},
		{plain, `["a",1]`, "2.5\n", false},
	} {
		if got, f := translatedAs(t, c.from, "json", c.path, nil, tree, c.whole); f != nil || got != c.want {
			t.Errorf("%s (whole %v): got %q (%v), want %q", c.path, c.whole, got, f, c.want)
		}
	}
	if got, want := mustTranslate(t, through, "csv", `["rows"]`, echo, `{"rows":`+records+`}`), mustTranslate(t, plain, "csv", "", echo, records); got != want {
		t.Errorf("a path's records with a program: got %q, want %q", got, want)
	}
	for _, c := range []struct {
		from *Format
		path string
	}{
		{through, `["a",4]`},
		{through, `["a","0"]`},
		{through, `["c",0]`},
		{through, `["f","g"]`},
		{plain, `["z"]`},
		{marking, `["a"]`},
	} {
		_, f := translated(t, c.from, "json", c.path, nil, tree)
		if f == nil || f.Code != tt.CodeInputInvalid || f.Message != "the path "+c.path+" names nothing in the document" || f.CommittedOutput {
			t.Errorf("%s: %v", c.path, f)
		}
	}
	repeated := `{"a":1,"a":2}`
	if _, f := translated(t, through, "json", "", nil, repeated); f == nil || f.Code != tt.CodeDuplicateMember || !strings.HasSuffix(f.Message, "; --whole reads the document whole") {
		t.Errorf("a repeated member, streamed: %v", f)
	}
	whole, f := translatedAs(t, through, "json", "", nil, repeated, true)
	if want := mustTranslate(t, plain, "json", "", nil, repeated); f != nil || whole != want || whole != "{\"a\":2}\n" {
		t.Errorf("a repeated member, read whole: the transducer's format wrote %q (%v), the plain one %q", whole, f, want)
	}
	// Under a path, a repeated member fails as it does at the root, inside
	// the value, on the way to it or past it, since the grammar's value
	// decides which of the two a path names; read whole, the path selects in
	// it.
	around := `{"x":{"a":1,"a":2},"y":3}`
	for _, path := range []string{`["x"]`, `["x","a"]`, `["y"]`} {
		if _, f := translated(t, through, "json", path, nil, around); f == nil || f.Code != tt.CodeDuplicateMember || !strings.HasSuffix(f.Message, "; --whole reads the document whole") {
			t.Errorf("%s, a repeated member, streamed: %v", path, f)
		}
	}
	for path, want := range map[string]string{`["x"]`: "{\"a\":2}\n", `["x","a"]`: "2\n"} {
		if got, f := translatedAs(t, through, "json", path, nil, around, true); f != nil || got != want {
			t.Errorf("%s, a repeated member, read whole: got %q (%v), want %q", path, got, f, want)
		}
	}
	if _, f := translated(t, through, "json", "", nil, "[1,"); f == nil || f.Code != tt.CodeInputInvalid {
		t.Errorf("a refused document: %v", f)
	}
}

// YAML is read through its part, and a repeated key, which the part
// streams twice and the tree contract refuses, fails unless the document
// is read whole: the grammar merges it, at the root and in a sequence
// entry's mapping alike (tabnas/yaml 0.5.26 merges the entry's, which kept
// the later value before). A sequence entry's keys stand at the column its
// content starts at, so a key with an empty value before a sibling is null,
// and an entry with more than one space after its dash reads (both 0.5.26).
func TestYAMLReadsASequenceEntryAsItsGrammarDoes(t *testing.T) {
	yaml := Named("yaml")
	for _, c := range []struct {
		doc, want string
		repeats   bool
	}{
		{"a:\n  b: 1\na:\n  c: 2\n", "{\"a\":{\"b\":1,\"c\":2}}\n", true},
		{"- a:\n    b: 1\n  a:\n    c: 2\n", "[{\"a\":{\"b\":1,\"c\":2}}]\n", true},
		{"- a:\n  b: 1\n", "[{\"a\":null,\"b\":1}]\n", false},
		{"-   a: 1\n    b: 2\n", "[{\"a\":1,\"b\":2}]\n", false},
	} {
		if c.repeats {
			if _, f := translated(t, yaml, "json", "", nil, c.doc); f == nil || f.Code != tt.CodeDuplicateMember {
				t.Errorf("%q, streamed: %v", c.doc, f)
			}
		}
		if got, f := translatedAs(t, yaml, "json", "", nil, c.doc, c.repeats); f != nil || got != c.want {
			t.Errorf("%q: got %q (%v), want %q", c.doc, got, f, c.want)
		}
	}
}

// Every format with a parser answers its rule passes; one read as a tree
// has none and refuses; json5's check runs first, over a text and through
// a reader.
func TestAFormatWithAParserGivesItsRulePasses(t *testing.T) {
	var parsed []string
	for _, f := range Formats() {
		if f.Parses() {
			parsed = append(parsed, f.ID())
		}
	}
	// C is parsed here, where the Rust crate reads it as its realized
	// tree; feed, pgn and proto are parsed for their transducer parts, and a
	// document read whole is read as the typed feed's or database's tree,
	// or proto's source-ordered tree.
	want := "abnf c css csv ebnf feed gbnf ini json json5 jsonc jsonic jsonl markdown pgn proto semver toml xml yaml zon"
	if got := strings.Join(parsed, " "); got != want {
		t.Errorf("the parsed formats are %s", got)
	}
	if got := ParserNames(); got != "abnf, c, css, csv, ebnf, feed, gbnf, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, proto, semver, toml, xml, yaml or zon" {
		t.Errorf("ParserNames is %q", got)
	}
	expr := Named("expr")
	if expr.Parses() {
		t.Error("expr is read as a tree")
	}
	if _, f := expr.Passes("1+2"); f == nil || f.Code != tt.CodeInputInvalid || !strings.HasPrefix(f.Message, "expr is read as a tree") {
		t.Errorf("a tree has no passes: %v", f)
	}
	json5 := Named("json5")
	if _, f := json5.Passes("// only a comment"); f == nil || f.Code != tt.CodeInputInvalid {
		t.Errorf("the check refuses: %v", f)
	}
	if _, f := json5.PassesFrom(strings.NewReader("// only a comment")); f == nil || f.Code != tt.CodeInputInvalid {
		t.Errorf("the check refuses through a reader: %v", f)
	}
	if source, f := json5.Passes("{a: 1}"); f != nil || source == nil {
		t.Errorf("the check passes: %v", f)
	}
}
