// Copyright (c) 2026 tabnas, MIT License

package main

// translate_test.go: dmog translate and dmog formats, run the way a
// script runs them (cli_test.go builds the binary): the flags, the paths,
// the programs, the failures and their statuses, and that standard error
// carries nothing on success. The bytes expected are the Rust binary's,
// which writes the same for the same arguments.
//
// translate reads a JSON document through json's transducer part, over the
// parser's rule passes, in every build, so a number is written as the
// document spelled it (30.50); a path keeps the document's value, read
// whole, which spells a number by its value (30.5).

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"testing"

	tabnasabnf "github.com/tabnas/abnf/go"
	tabnasc "github.com/tabnas/c/go"
	tabnascss "github.com/tabnas/css/go"
	"github.com/tabnas/dmog/go/translate"
	tabnasebnf "github.com/tabnas/ebnf/go"
	tabnasexpr "github.com/tabnas/expr/go"
	tabnasfeed "github.com/tabnas/feed/go"
	tabnasgbnf "github.com/tabnas/gbnf/go"
	tabnasini "github.com/tabnas/ini/go"
	tabnasmarkdown "github.com/tabnas/markdown/go"
	tabnasproto "github.com/tabnas/proto/go"
	tabnassemver "github.com/tabnas/semver/go"
	tabnastoml "github.com/tabnas/toml/go"
)

// people is a document with a nested table and numbers whose lexeme is not
// their shortest text.
const people = `{"people":[{"name":"Ann","age":30.50},{"name":"Bo","age":7}],"n":1.50}`

// wantSuccess is a run that wrote want and nothing on standard error.
func wantSuccess(t *testing.T, o output, want string) {
	t.Helper()
	if o.status != 0 || o.stdout != want || o.stderr != "" {
		t.Errorf("status %d, stdout %q, stderr %q; want %q", o.status, o.stdout, o.stderr, want)
	}
}

// wantFailure is a run that failed with status and code, the message
// beginning with prefix, and nothing on standard output.
func wantFailure(t *testing.T, o output, status int, code, prefix string) map[string]any {
	t.Helper()
	f := failJSON(t, o)
	message, _ := f["message"].(string)
	if o.status != status || o.stdout != "" || f["code"] != code || !strings.HasPrefix(message, prefix) {
		t.Errorf("status %d, stdout %q, failure %v; want status %d, %s, %q", o.status, o.stdout, f, status, code, prefix)
	}
	return f
}

func TestFormatsPrintsTheRegistryAsJSON(t *testing.T) {
	o := invoke(t, []string{"formats"}, nil)
	if o.status != 0 || o.stderr != "" || o.stdout != translate.FormatsJSON()+"\n" || strings.Count(o.stdout, "\n") != 1 {
		t.Fatalf("%+v", o)
	}
	// Each format's fields in their order, abnf's whole up to its loss.
	// Its whole is its manifest's sentence, null where it gives none.
	if !strings.HasPrefix(o.stdout, `[{"id":"abnf","reads":["tree"],"writes":"tree","root":"object","schema":"grammar-spec","whole":`+
		wholeJSON(t, tabnasabnf.Translate().Manifest)+`,"lift":null,"embed":null,"render":"abnf-render","transduce":null,"loss":["`) {
		t.Errorf("%s", o.stdout)
	}
	var formats []map[string]any
	if err := json.Unmarshal([]byte(o.stdout), &formats); err != nil {
		t.Fatal(err)
	}
	var ids []string
	for _, f := range formats {
		ids = append(ids, f["id"].(string))
		if len(f) != 11 {
			t.Errorf("%s: %v", f["id"], f)
		}
		loss, _ := f["loss"].([]any)
		if len(loss) == 0 {
			t.Errorf("%s declares no loss", f["id"])
		}
		for _, sentence := range loss {
			if s, ok := sentence.(string); !ok || s == "" {
				t.Errorf("%s: loss %v", f["id"], loss)
			}
		}
	}
	if strings.Join(ids, " ") != "abnf c css csv ebnf expr feed gbnf ini json json5 jsonc jsonic jsonl markdown pgn proto semver toml xml yaml zon" {
		t.Errorf("%v", ids)
	}
	byID := map[string]map[string]any{}
	for _, f := range formats {
		byID[f["id"].(string)] = f
	}
	if md := byID["markdown"]; md["lift"] != "markdown-lift" || md["schema"] != "markdown-ast" || md["render"] != "markdown-render" ||
		strings.Join([]string{md["reads"].([]any)[0].(string), md["reads"].([]any)[1].(string)}, " ") != "records tree" {
		t.Errorf("%v", md)
	}
	if xml := byID["xml"]; xml["embed"] != "xml-embed" || xml["schema"] != "xml-element" || xml["writes"] != "tree" {
		t.Errorf("%v", xml)
	}
	if toml := byID["toml"]; toml["root"] != "object" || toml["render"] != "toml-render" {
		t.Errorf("%v", toml)
	}
	// The formats that write a schema's tree with no embedding into it are
	// schema-only: the composition refuses any other format's tree for them.
	var schemaOnly []string
	for _, f := range formats {
		if f["schema"] != nil && f["writes"] == "tree" && f["embed"] == nil {
			schemaOnly = append(schemaOnly, f["id"].(string))
		}
	}
	if strings.Join(schemaOnly, " ") != "abnf c css ebnf gbnf pgn proto" {
		t.Errorf("%v", schemaOnly)
	}
	// The grammar notations share a schema, the grammar spec their
	// compilers emit, so each writes the others' documents.
	for _, id := range []string{"abnf", "ebnf", "gbnf"} {
		if byID[id]["schema"] != "grammar-spec" {
			t.Errorf("%s: %v", id, byID[id])
		}
	}
	if byID["c"]["schema"] != "c" {
		t.Errorf("%v", byID["c"])
	}
	// Why a format's documents are read whole, where its manifest says:
	// every format transduce's Rust and TypeScript lists leave out, and
	// Markdown, which they stream only once its last line is in, each in
	// the sentence its manifest gives. PGN's says nothing: it streams a
	// game at a time through its transducer part, here as there.
	for id, manifest := range map[string]string{
		"abnf":     tabnasabnf.Translate().Manifest,
		"c":        tabnasc.Translate().Manifest,
		"ebnf":     tabnasebnf.Translate().Manifest,
		"expr":     tabnasexpr.Translate().Manifest,
		"gbnf":     tabnasgbnf.Translate().Manifest,
		"markdown": tabnasmarkdown.Translate().Manifest,
		"proto":    tabnasproto.Translate().Manifest,
		"semver":   tabnassemver.Translate().Manifest,
	} {
		var m struct {
			Translate struct {
				Whole string `json:"whole"`
			} `json:"translate"`
		}
		if err := json.Unmarshal([]byte(manifest), &m); err != nil || m.Translate.Whole == "" || byID[id]["whole"] != m.Translate.Whole {
			t.Errorf("%s: whole %v, its manifest's %q (%v)", id, byID[id]["whole"], m.Translate.Whole, err)
		}
	}
	// CSS, INI, TOML and feed give the sentence up with the transducer
	// part they ship from their next release, so each is held to the module
	// this build has, whichever side of that release: the published one, or
	// the sibling checkout the polyglot gate's workspace uses.
	for id, manifest := range map[string]string{
		"css":  tabnascss.Translate().Manifest,
		"feed": tabnasfeed.Translate().Manifest,
		"ini":  tabnasini.Translate().Manifest,
		"toml": tabnastoml.Translate().Manifest,
	} {
		var m struct {
			Translate struct {
				Whole *string `json:"whole"`
			} `json:"translate"`
		}
		if err := json.Unmarshal([]byte(manifest), &m); err != nil {
			t.Fatal(err)
		}
		if m.Translate.Whole == nil {
			if byID[id]["whole"] != nil {
				t.Errorf("%s: whole %v, its manifest gives none", id, byID[id]["whole"])
			}
		} else if byID[id]["whole"] != *m.Translate.Whole {
			t.Errorf("%s: whole %v, its manifest's %q", id, byID[id]["whole"], *m.Translate.Whole)
		}
	}
	for _, id := range []string{"json", "pgn"} {
		if byID[id]["whole"] != nil {
			t.Errorf("%s: whole %v", id, byID[id]["whole"])
		}
	}
	if byID["pgn"]["schema"] != "pgn-database" || byID["semver"]["embed"] != "semver-embed" ||
		byID["feed"]["embed"] != "feed-embed" || byID["expr"]["embed"] != "expr-embed" {
		t.Errorf("%v %v %v %v", byID["pgn"], byID["semver"], byID["feed"], byID["expr"])
	}
	if o := invoke(t, []string{"formats", "x"}, nil); o.status != 2 || !strings.Contains(failJSON(t, o)["message"].(string), "usage:") {
		t.Errorf("%+v", o)
	}
}

// wholeJSON is a manifest's translate.whole as `dmog formats` writes
// it: the sentence as a JSON string, or null where the manifest gives none.
func wholeJSON(t *testing.T, manifest string) string {
	t.Helper()
	var m struct {
		Translate struct {
			Whole *string `json:"whole"`
		} `json:"translate"`
	}
	if err := json.Unmarshal([]byte(manifest), &m); err != nil {
		t.Fatal(err)
	}
	if m.Translate.Whole == nil {
		return "null"
	}
	var b strings.Builder
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(*m.Translate.Whole); err != nil {
		t.Fatal(err)
	}
	return strings.TrimSuffix(b.String(), "\n")
}

// A document read with --from's grammar and written in --to's format,
// from a file or from standard input; nothing reaches standard error,
// though a translation loses what the target's loss sentences say (in
// dmog formats), as in the Rust binary.
func TestTranslateWritesADocumentInAnotherFormat(t *testing.T) {
	input := tempFile(t, "people.json", people)
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", input}, nil),
		"\"people\":\n  - \"name\": \"Ann\"\n    \"age\": 30.50\n  - \"name\": \"Bo\"\n    \"age\": 7\n\"n\": 1.50\n")
	// A later flag replaces an earlier one.
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--to", "json", input}, nil),
		people+"\n")
	for _, c := range []struct{ from, to, stdin, want string }{
		{"json", "json", `[1.50,1e2,-0.0]`, "[1.50,1e2,-0.0]" + "\n"},
		{"csv", "json", "a,b\n1,x\n2,y\n", `[{"a":"1","b":"x"},{"a":"2","b":"y"}]` + "\n"},
		// Markdown read as records through its lift, into a records render.
		{"markdown", "csv", "| a | b |\n| - | - |\n| 1 | x |\n", "\"a\",\"b\"\r\n\"1\",\"x\"\r\n"},
		// A root that is not an array, as the one element of one.
		{"json", "jsonl", `{"a":1}`, `{"a":1}` + "\n"},
		{"json", "jsonl", `[{"a":1},2]`, "{\"a\":1}\n2\n"},
		// Lossy: TOML has no null, and writes the root under the key.
		{"json", "toml", `[1,null]`, "\"items\" = [ 1 ]\n"},
	} {
		wantSuccess(t, invoke(t, []string{"translate", "--from", c.from, "--to", c.to, "-"}, text(c.stdin)), c.want)
	}
	// The values a transducer part cannot follow, a YAML stream of several
	// documents and a repeated member: the output streams, so each fails
	// with its code and names the route that reads it, and --whole reads it
	// whole, as its grammar's value (admin ADR-33).
	for _, c := range []struct {
		from, stdin, code string
		status            int
		want              string
	}{
		{"yaml", "a: 1\n---\nb: 2\n", "STREAMABILITY_UNKNOWN", 2, `[{"a":1},{"b":2}]` + "\n"},
		{"json", `{"a":1,"a":2}`, "DUPLICATE_MEMBER", 1, `{"a":2}` + "\n"},
	} {
		f := wantFailure(t, invoke(t, []string{"translate", "--from", c.from, "--to", "json", "-"}, text(c.stdin)), c.status, c.code, "")
		if message, _ := f["message"].(string); !strings.HasSuffix(message, "; --whole reads the document whole") {
			t.Errorf("%s: %v", c.stdin, f)
		}
		wantSuccess(t, invoke(t, []string{"translate", "--whole", "--from", c.from, "--to", "json", "-"}, text(c.stdin)), c.want)
	}
	// Under a path as at the root; read whole, the path selects in the
	// grammar's value.
	for _, c := range []struct {
		from, path, wholePath, stdin, code string
		status                             int
		want                               string
	}{
		{"yaml", `["b"]`, `[1,"b"]`, "a: 1\n---\nb: 2\n", "STREAMABILITY_UNKNOWN", 2, "2\n"},
		{"json", `["b"]`, `["a"]`, `{"a":1,"a":2,"b":3}`, "DUPLICATE_MEMBER", 1, "2\n"},
	} {
		f := wantFailure(t, invoke(t, []string{"translate", "--from", c.from, "--to", "json", "--path", c.path, "-"}, text(c.stdin)), c.status, c.code, "")
		if message, _ := f["message"].(string); !strings.HasSuffix(message, "; --whole reads the document whole") {
			t.Errorf("%s at %s: %v", c.stdin, c.path, f)
		}
		wantSuccess(t, invoke(t, []string{"translate", "--whole", "--from", c.from, "--to", "json", "--path", c.wholePath, "-"}, text(c.stdin)), c.want)
	}
}

// --key names the member a root is wrapped under for a format whose
// document is an object; --path translates the value it names, whatever the
// source's shapes: a sub-stream of the document's events, so a number keeps
// the lexeme the document spelled it with, or, read whole with --whole, in
// the grammar's value.
func TestTranslateTakesAKeyAndAPath(t *testing.T) {
	input := tempFile(t, "people.json", people)
	translateTo := func(to string, flags ...string) output {
		return invoke(t, append(append([]string{"translate", "--from", "json", "--to", to}, flags...), input), nil)
	}
	table := `[ { "name" = "Ann", "age" = 30.50 }, { "name" = "Bo", "age" = 7 } ]` + "\n"
	wantSuccess(t, translateTo("toml", "--path", `["people"]`), `"items" = `+table)
	wantSuccess(t, translateTo("toml", "--key", "rows", "--path", `["people"]`), `"rows" = `+table)
	wantSuccess(t, translateTo("csv", "--path", `["people"]`), "\"name\",\"age\"\r\n\"Ann\",\"30.50\"\r\n\"Bo\",\"7\"\r\n")
	wantSuccess(t, translateTo("markdown", "--path", `["people"]`), "| name | age |\n| --- | --- |\n| Ann | 30.50 |\n| Bo | 7 |\n")
	wantSuccess(t, translateTo("yaml", "--path", `["people",0]`), "\"name\": \"Ann\"\n\"age\": 30.50\n")
	wantSuccess(t, translateTo("ini", "--path", `["n"]`), "\"items\" = 1.50\n")
	wantSuccess(t, translateTo("json", "--path", `[]`), people+"\n")
	// Read whole, the path selects in the grammar's value.
	wantSuccess(t, translateTo("csv", "--whole", "--path", `["people"]`), "\"name\",\"age\"\r\n\"Ann\",\"30.5\"\r\n\"Bo\",\"7\"\r\n")
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "toml", "--key", "", "-"}, text("1.50")),
		`"" = 1.50`+"\n")
	// A path that names nothing is the document's failure, status 1.
	f := wantFailure(t, translateTo("yaml", "--path", `["people",5]`), 1, "INPUT_INVALID", "")
	if f["message"] != `the path ["people",5] names nothing in the document` {
		t.Errorf("%v", f)
	}
	// A path that is not one is the command line's, status 2.
	for _, path := range []string{`{}`, `"people"`, `[1.5]`, `[-1]`, `[true]`, `[null]`, ``, `[1`} {
		f := wantFailure(t, translateTo("yaml", "--path", path), 2, "INPUT_INVALID", "")
		if f["message"] != `--path takes a JSON array of keys and indexes, such as ["people",0], not `+path {
			t.Errorf("%q: %v", path, f)
		}
	}
	// A whole number is an index, however it is spelled.
	wantSuccess(t, translateTo("json", "--path", `["people",1e0,"age"]`), "7\n")
}

// --with runs a program over the input first, and its export's JSON
// events or table stand in the source's place; a program that writes its
// own text, or does not check, is refused before the input is read.
func TestTranslateRunsAProgramFirst(t *testing.T) {
	input := tempFile(t, "people.json", people)
	echo := tempFile(t, "echo.alc", "def export [input] input\n")
	table := tempFile(t, "table.alc", "def binding\n  record\n    entry :columns :infer\n    entry :rows (path \"people\" each-index)\n\n"+
		"def export [input] (table-from-json binding input)\n")
	ownText := tempFile(t, "text.alc", "def export [input] (csv csv-options (table-from-json (record (entry :columns :infer) (entry :rows (path each-index))) input))\n")
	broken := tempFile(t, "broken.alc", "def export [input] (nope input)\n")
	age := "30.50"
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "csv", "--with", table, input}, nil),
		"\"name\",\"age\"\r\n\"Ann\",\""+age+"\"\r\n\"Bo\",\"7\"\r\n")
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "json", "--with", table, input}, nil),
		`[{"name":"Ann","age":`+age+`},{"name":"Bo","age":7}]`+"\n")
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", "-", input}, text("def export [input] input\n")),
		"\"people\":\n  - \"name\": \"Ann\"\n    \"age\": "+age+"\n  - \"name\": \"Bo\"\n    \"age\": 7\n\"n\": 1.50\n")
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", echo, "-"}, text("[1]")), "- 1\n")
	// Refused before the input is read: an input that cannot be read is
	// never reached.
	f := wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", ownText, "/nonexistent/input.json"}, nil),
		2, "DSL_TYPE_ERROR", "bad_output: ")
	if f["message"] != "bad_output: "+ownText+"'s export writes its own text, and translate takes a program whose export answers JSON events or a table, which a format's render then writes" {
		t.Errorf("%v", f)
	}
	wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", broken, "/nonexistent/input.json"}, nil),
		2, "DSL_TYPE_ERROR", "unknown_name: nope")
	wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", "/nonexistent/program.alc", input}, nil),
		2, "INPUT_INVALID", "cannot read /nonexistent/program.alc: ")
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--with", "-", "-"}, text("[1]")), 2, "INPUT_INVALID", "")
	if f["message"] != "only one of PROGRAM and INPUT may be - (standard input)" {
		t.Errorf("%v", f)
	}
}

// What a target declares it cannot carry it refuses before writing
// anything, status 1: a schema-only target refuses another format's tree
// when the route is composed, before the input is read, a grammar
// notation's render refuses a grammar spec it has no form for, naming what
// it met, and Semantic Versioning's embedding refuses a tree that is not a
// version. Each takes its own format's documents, and a program that makes
// its tree; a grammar notation takes the others' too.
func TestTranslateRefusesWhatATargetCannotCarry(t *testing.T) {
	f := wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "css", "/nonexistent/input.json"}, nil),
		1, "TARGET_VALUE_UNREPRESENTABLE", "")
	if f["message"] != "schema_only: css writes a css-ast tree, the tree its own documents read as, and this document is "+
		"not one; a program that makes one can be composed with the render" || f["output"] != "none" || len(f) != 3 {
		t.Errorf("%v", f)
	}
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "semver", "-"}, text(`{"major":1,"minor":2}`)),
		1, "TARGET_VALUE_UNREPRESENTABLE", "")
	if f["message"] != "the document is not a version: it has no patch; a version is an object whose major, minor and "+
		"patch are whole numbers written in digits, with an optional prerelease and build, each a list of identifiers "+
		"or one string of them" || f["row"] != 48.0 || f["col"] != 3.0 || f["file"] != "tabnas-semver/alchemy/embed.alc" {
		t.Errorf("%v", f)
	}
	for _, c := range []struct{ to, schema string }{{"abnf", "grammar-spec"}, {"c", "c"}} {
		f = wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", c.to, "-"}, text("{}")),
			1, "TARGET_VALUE_UNREPRESENTABLE", "")
		if f["message"] != "schema_only: "+c.to+" writes a "+c.schema+" tree, the tree its own documents read as, and "+
			"this document is not one; a program that makes one can be composed with the render" || len(f) != 3 {
			t.Errorf("%s: %v", c.to, f)
		}
	}
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "abnf", "--to", "ebnf", "-"}, text("greet = \"hi\"\n")),
		1, "TARGET_VALUE_UNREPRESENTABLE", "")
	if f["message"] != "the grammar spec cannot be written as EBNF: the case-insensitive literal \"hi\" is no one W3C "+
		"EBNF terminal: it would be written as [hH] [iI], and this front end reads whitespace between terminals" ||
		f["row"] != 243.0 || f["col"] != 3.0 || f["file"] != "tabnas-ebnf/alchemy/render.alc" {
		t.Errorf("%v", f)
	}
	echo := tempFile(t, "echo-css.alc", "def export [input] input\n")
	tree := `{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":` +
		`[{"type":"declaration","property":"color","value":"red"}]}]}`
	for _, c := range []struct {
		args        []string
		stdin, want string
	}{
		{[]string{"--from", "json", "--to", "semver", "-"}, `{"major":1,"minor":2,"patch":3,"prerelease":"rc.1"}`, "1.2.3-rc.1"},
		{[]string{"--from", "css", "--to", "css", "-"}, "a{color:red}", "a {\n  color: red;\n}\n"},
		{[]string{"--from", "json", "--to", "css", "--with", echo, "-"}, tree, "a {\n  color: red;\n}\n"},
		{[]string{"--from", "json", "--to", "expr", "-"}, `{"a":[1,-2]}`, `{"a":[1,-2]}` + "\n"},
		{[]string{"--from", "expr", "--to", "json", "-"}, "1+2*3\n", `["+",1,["*",2,3]]` + "\n"},
		{[]string{"--from", "pgn", "--to", "pgn", "-"}, "1. e4 e5 1-0\n", "1. e4 e5 1-0\n"},
		{[]string{"--from", "proto", "--to", "proto", "-"}, "syntax = \"proto3\";\nmessage M { int32 a = 1; }\n",
			"syntax = \"proto3\";\nmessage M {\n  int32 a = 1;\n}\n"},
		// White space after the last token is not in the tree (C's loss
		// list), so the final line feed is not written.
		{[]string{"--from", "c", "--to", "c", "-"}, "int main(void) { return 0; }\n", "int main(void) { return 0; }"},
		{[]string{"--from", "ebnf", "--to", "abnf", "-"}, "top ::= \"a\" b\nb ::= \"c\"\n", "top = %s\"a\" b\nb = %s\"c\"\n"},
		{[]string{"--from", "ebnf", "--to", "gbnf", "-"}, "top ::= \"a\" b\nb ::= \"c\"\n", "root ::= top\ntop ::= \"a\" b\nb ::= \"c\"\n"},
		{[]string{"--from", "abnf", "--to", "ebnf", "-"}, "top = \"a\"\n", "top ::= [aA]\n"},
	} {
		wantSuccess(t, invoke(t, append([]string{"translate"}, c.args...), text(c.stdin)), c.want)
	}
}

// What a module's own parse checks before its parser runs, the command
// checks too, status 1, writing nothing, as the Rust and TypeScript
// commands do: a JSON5 document holding no value is refused with json5's
// own codes (its Parse), not the engine's unexpected; a .proto file nesting
// past proto's cap is refused before the engine runs (the Preflight its
// ParseValue makes); and a grammar notation's document its compiler
// refuses is refused with the compiler's message.
func TestTranslateChecksWhatAModulesParseChecksFirst(t *testing.T) {
	for _, c := range []struct{ input, code string }{{"", "json5_empty"}, {"// c\n", "json5_no_value"}} {
		f := wantFailure(t, invoke(t, []string{"translate", "--from", "json5", "--to", "json", "-"}, text(c.input)),
			1, "INPUT_INVALID", c.code+": JSON5 input must contain a value")
		if f["row"] != 1.0 || f["col"] != 1.0 || f["output"] != "none" {
			t.Errorf("%q: %v", c.input, f)
		}
	}
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json5", "--to", "json", "-"}, text("// c\n{a:1}\n")), `{"a":1}`+"\n")
	deep := func(n int) *string {
		return text("syntax = \"proto3\";\n" + strings.Repeat("message M {", n) + strings.Repeat("}", n) + "\n")
	}
	f := wantFailure(t, invoke(t, []string{"translate", "--from", "proto", "--to", "json", "-"}, deep(101)),
		1, "INPUT_INVALID", "")
	if f["message"] != "proto: document nests 101 levels deep, past the 100 this parser accepts" || len(f) != 3 {
		t.Errorf("101 deep: %v", f)
	}
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "abnf", "--to", "json", "-"}, text("a = \"b\" c\n")),
		1, "INPUT_INVALID", "")
	if f["message"] != "abnf: rule 'a' references unknown rule 'c'" || len(f) != 3 {
		t.Errorf("%v", f)
	}
	// A left recursion through a cycle of twenty rules, each with two
	// recursive alternates, would make tabnas-bnf's elimination substitute
	// 2^20 alternates. tabnas-bnf bounds what a rule gains at 512
	// alternatives, so the grammar is refused in moments rather than
	// aborting.
	const k = 20
	var cycle strings.Builder
	cycle.WriteString("root ::= r0\n")
	for i := 0; i < k; i++ {
		next := (i + 1) % k
		fmt.Fprintf(&cycle, "r%d ::= r%d \"a%d\" | r%d \"b%d\" | \"x%d\"\n", i, next, i, next, i, i)
	}
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "gbnf", "--to", "json", "-"}, text(cycle.String())),
		1, "INPUT_INVALID", "")
	if f["message"] != "gbnf: rule 'r11' exceeds the left-recursion expansion limit of 512 alternatives while "+
		"inlining 'r12', which begins 2 of its alternatives and has 511 of its own. Factor 'r12' out of the "+
		"alternatives it begins." || len(f) != 3 {
		t.Errorf("the left-recursion cycle: %v", f)
	}
}

// The statuses follow the code, and every failure is one JSON object on
// standard error with nothing on standard output: what a run writes is
// held until it has succeeded.
func TestTranslateFailuresAndStatuses(t *testing.T) {
	input := tempFile(t, "people.json", people)
	for _, args := range [][]string{
		{"translate"},
		{"translate", "--from", "json", input},
		{"translate", "--to", "json", input},
		{"translate", "--from", "json", "--to", "yaml"},
		{"translate", "--from", "json", "--to", "yaml", input, input},
		{"translate", "--from", "json", "--to", "yaml", "--bogus", input},
		{"translate", "--from", "json", "--to"},
		{"translate", "--bogus", "--from", "bogus", input},
	} {
		f := wantFailure(t, invoke(t, args, nil), 2, "INPUT_INVALID", "usage: ")
		if !strings.Contains(f["message"].(string), "dmog translate --from FORMAT --to FORMAT") {
			t.Errorf("%v: %v", args, f)
		}
	}
	names := "abnf, c, css, csv, ebnf, expr, feed, gbnf, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, proto, semver, toml, " +
		"xml, yaml or zon"
	for _, c := range []struct {
		args []string
		want string
	}{
		{[]string{"--from", "bogus", "--to", "yaml"}, `--from takes ` + names + `, not "bogus"`},
		// Found where it stands, before the usage error after it.
		{[]string{"--to", "Yaml", "--bogus"}, `--to takes ` + names + `, not "Yaml"`},
		// Quoted as the Rust binary quotes it.
		{[]string{"--from", "json", "--to", "a\tb\x01\u0301\"\\"}, `--to takes ` + names + `, not "a\tb\u{1}\u{301}\"\\"`},
		{[]string{"--from", "json", "--to", "yaml", "--max-output-bytes", "many"}, `--max-output-bytes takes a number of bytes, not "many"`},
	} {
		f := wantFailure(t, invoke(t, append(append([]string{"translate"}, c.args...), input), nil), 2, "INPUT_INVALID", "")
		if f["message"] != c.want {
			t.Errorf("%v: %v", c.args, f)
		}
	}
	f := wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "/nonexistent/input.json"}, nil), 2, "INPUT_INVALID", "")
	if f["message"] != "cannot read /nonexistent/input.json: No such file or directory (os error 2)" {
		t.Errorf("%v", f)
	}
	// A document its grammar refuses: status 1, with the engine's position.
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "json", "-"}, text(`{"a":`)), 1, "INPUT_INVALID", "unexpected: ")
	if f["row"] != 1.0 || f["col"] != 6.0 || f["output"] != "none" {
		t.Errorf("%v", f)
	}
	// An output longer than --max-output-bytes: status 5, the limit named,
	// and nothing written.
	f = wantFailure(t, invoke(t, []string{"translate", "--from", "json", "--to", "yaml", "--max-output-bytes", "10", input}, nil),
		5, "RESOURCE_LIMIT_EXCEEDED", "the output would exceed 10 bytes")
	if limit, _ := f["limit"].(map[string]any); limit["name"] != "max_output_bytes" || f["output"] != "none" {
		t.Errorf("%v", f)
	}
	wantSuccess(t, invoke(t, []string{"translate", "--from", "json", "--to", "json", "--max-output-bytes", "+100", "-"}, text(`[1]`)), "[1]\n")
}

// A standard output that cannot be written is OUTPUT_FAILED, status 3,
// with the error the Rust binary's message names.
func TestTranslateToAClosedStandardOutputIsOutputFailed(t *testing.T) {
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	r.Close()
	cmd := exec.Command(binary, "translate", "--from", "json", "--to", "yaml", "-")
	cmd.Stdin = strings.NewReader(people)
	cmd.Stdout = w
	var stderr strings.Builder
	cmd.Stderr = &stderr
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	w.Close()
	err = cmd.Wait()
	var exit *exec.ExitError
	if !errors.As(err, &exit) || exit.ExitCode() != 3 {
		t.Fatalf("%v", err)
	}
	f := failJSON(t, output{stderr: stderr.String()})
	if f["code"] != "OUTPUT_FAILED" || f["message"] != "writing the output failed: Broken pipe (os error 32)" {
		t.Errorf("%v", f)
	}
}
