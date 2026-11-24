// Copyright (c) 2026 tabnas, MIT License

package e2e

// generated_test.go: the transducer part tabnas-abnf generates for an ABNF
// grammar (step 8 of alchemy's docs/streaming-plan.md), run as a host runs
// it (rs/tests/generated_part_test.rs): compiled by alchemy, over the
// grammar's rule passes from transduce's ParserEventSource. Over every
// grammar of abnf's alignment-abnf-ast.tsv, a generated part writes the
// value the compiled grammar builds, and the
// generator declares every other grammar read whole, for the reason it
// gives for a start rule with no annotation. The part's events reach a
// renderer the program builds (ParserSink) and a sink of the host's own
// (ParserSinkInto), which is how aless runs a part in front of its own
// stages.

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	tabnasabnf "github.com/tabnas/abnf/go"
	. "github.com/tabnas/alchemy/go"
	"github.com/tabnas/alchemy/go/shared"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
	tt "github.com/tabnas/transduce/go"
)

// abnfSpecDir is abnf's shared fixture directory, vendored.
func abnfSpecDir(t testing.TB) string {
	t.Helper()
	dir := filepath.Join(repoRoot(t), "..", "abnf", "test", "spec")
	if _, err := os.Stat(dir); err != nil {
		t.Fatalf("abnf's fixtures are vendored, at %s: %v", dir, err)
	}
	return dir
}

// abnfEngine is an engine carrying the grammar, as abnf's own tests
// install one.
func abnfEngine(t testing.TB, spec *tabnas.GrammarSpec) *tabnas.Tabnas {
	t.Helper()
	history := 4096
	j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &history}})
	if err := j.Grammar(spec); err != nil {
		t.Fatal(err)
	}
	return j
}

// asJSON is a value as JSON reads it back, for a comparison of values.
func asJSON(t testing.TB, raw []byte) any {
	t.Helper()
	var out any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("not JSON: %v: %s", err, raw)
	}
	return out
}

// generatedCase is one grammar of the fixture whose generated part
// streams: its spec, a document, the value the grammar builds of it, and
// the part compiled.
type generatedCase struct {
	grammar, input string
	spec           *tabnas.GrammarSpec
	raw            []byte
	want           any
	program        *Program
}

// generatedCases is every grammar of abnf's alignment-abnf-ast.tsv with
// its generated part, each grammar the generator declares read whole held
// to the reason it gives for a start rule with no annotation, never an
// annotated one.
func generatedCases(t *testing.T) []generatedCase {
	t.Helper()
	fixture, err := support.LoadSpec(filepath.Join(abnfSpecDir(t), "alignment-abnf-ast.tsv"), nil)
	if err != nil {
		t.Fatal(err)
	}
	var cases []generatedCase
	for _, row := range fixture.Rows {
		grammar := row.UnescNamed("grammar")
		input := row.UnescNamed("input")
		annotated := strings.Contains(grammar, "@object") || strings.Contains(grammar, "@array")
		part, err := tabnasabnf.AbnfTransduce(grammar, "g", nil)
		if err != nil {
			t.Fatalf("%q converts: %v", grammar, err)
		}
		if part.Whole != "" {
			if annotated || !strings.HasPrefix(part.Whole, "the start rule ") {
				t.Errorf("%q is read whole: %s", grammar, part.Whole)
			}
			continue
		}
		spec, err := tabnasabnf.Abnf(grammar, nil)
		if err != nil {
			t.Fatal(err)
		}
		value, err := abnfEngine(t, spec).Parse(input)
		if err != nil {
			t.Fatalf("%q parses %q: %v", grammar, input, err)
		}
		raw, err := json.Marshal(value)
		if err != nil {
			t.Fatal(err)
		}
		src := part.Source + "\ndef export [input] (" + part.Entry + " input)\n"
		program, f := CompileWith(src, "generated.alc", InputParserEvents, routers, renderers)
		if f != nil {
			t.Fatalf("the part of %q compiles: %v\n%s", grammar, f, part.Source)
		}
		cases = append(cases, generatedCase{
			grammar: grammar,
			input:   input,
			spec:    spec,
			raw:     raw,
			want:    asJSON(t, raw),
			program: program,
		})
	}
	if len(cases) <= 10 {
		t.Errorf("%d grammars streamed", len(cases))
	}
	return cases
}

func TestAGeneratedPartWritesTheValueItsGrammarBuilds(t *testing.T) {
	var failures []string
	for _, c := range generatedCases(t) {
		var out bytes.Buffer
		sink, f := c.program.ParserSink(&out, RenderJSON, tt.DefaultLimits(), nil)
		if f == nil {
			_, f = tt.NewParserEventSource(abnfEngine(t, c.spec), c.input).Run(sink)
		}
		if f != nil || !reflect.DeepEqual(asJSON(t, out.Bytes()), c.want) {
			failures = append(failures, fmt.Sprintf("%q on %q\n  part:  %s %v\n  value: %s", c.grammar, c.input, out.String(), f, c.raw))
		}
	}
	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}
}

// TestAGeneratedPartsEventsReachAHostsOwnSink runs the parts again, their
// events into a sink of the host's own (ParserSinkInto), as aless runs a
// format's part in front of stages of its own: here render's JSON
// renderer, which the host builds itself, writes the value each grammar
// builds. The source's limits hold on the
// part's events, as on a source's own.
func TestAGeneratedPartsEventsReachAHostsOwnSink(t *testing.T) {
	limits := tt.DefaultLimits()
	cases := generatedCases(t)
	var failures []string
	for _, c := range cases {
		var out bytes.Buffer
		own := renderers.JSON(renderers.WriteOut(&out, limits, nil), shared.JSONOptions{TrailingNewline: true})
		sink, f := c.program.ParserSinkInto(own, limits, nil)
		if f == nil {
			_, f = tt.NewParserEventSource(abnfEngine(t, c.spec), c.input).Run(sink)
		}
		if f != nil || !reflect.DeepEqual(asJSON(t, out.Bytes()), c.want) {
			failures = append(failures, fmt.Sprintf("%q on %q\n  part:  %s %v\n  value: %s", c.grammar, c.input, out.String(), f, c.raw))
		}
	}
	if len(failures) > 0 {
		t.Error(strings.Join(failures, "\n"))
	}

	if len(cases) == 0 {
		return
	}
	c := cases[0]
	tight := tt.DefaultLimits()
	tight.MaxDepth = 0
	var out bytes.Buffer
	own := renderers.JSON(renderers.WriteOut(&out, tight, nil), shared.JSONOptions{TrailingNewline: true})
	sink, f := c.program.ParserSinkInto(own, tight, nil)
	if f == nil {
		_, f = tt.NewParserEventSource(abnfEngine(t, c.spec), c.input).Run(sink)
	}
	if f == nil || f.Limit == nil || f.Limit.Name != "max_depth" {
		t.Errorf("a part's events are held to the source's limits: %v", f)
	}
}
