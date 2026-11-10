// Copyright (c) 2026 tabnas, MIT License

// Fleet conformance for the package-local structural translation interface.
// The adapters below deliberately read every package's fields directly: that
// is the compile-time contract without introducing a shared package type.
// Each format's parts run round trip on transduce's routers and render's
// renderers. From alchemy's go/translation_parts_test.go, whose text-shape
// composition, which only compiles, stays there. The transducer part a
// module may ship (its manifest's translate.transduce) is held here too:
// present when declared and absent when not, under the entry the module
// names, read by the command, and composing over the format's parser's
// rule passes.

package e2e

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	. "github.com/tabnas/alchemy/go"
	altranslate "github.com/tabnas/alchemy/go/translate"
	tabnaschess "github.com/tabnas/chess/go"
	tabnascsv "github.com/tabnas/csv/go"
	translate "github.com/tabnas/dmog/go/translate"
	tabnasini "github.com/tabnas/ini/go"
	tabnasjson "github.com/tabnas/json/go"
	tabnasjson5 "github.com/tabnas/json5/go"
	tabnasjsonc "github.com/tabnas/jsonc/go"
	tabnasjsonic "github.com/tabnas/jsonic/go"
	tabnasjsonl "github.com/tabnas/jsonl/go"
	tabnasmarkdown "github.com/tabnas/markdown/go"
	tabnas "github.com/tabnas/parser/go"
	tabnastoml "github.com/tabnas/toml/go"
	tt "github.com/tabnas/transduce/go"
	tabnasxml "github.com/tabnas/xml/go"
	tabnasyaml "github.com/tabnas/yaml/go"
	tabnaszon "github.com/tabnas/zon/go"
)

type structuralPart struct {
	entry  string
	source string
}

type structuralParts struct {
	manifest string
	lift     *structuralPart
	render   *structuralPart
}

type translationDescriptor struct {
	LanguageID string `json:"languageId"`
	Translate  struct {
		Reads     any     `json:"reads"`
		Writes    string  `json:"writes"`
		Lift      *string `json:"lift"`
		Render    string  `json:"render"`
		Transduce *string `json:"transduce"`
	} `json:"translate"`
}

// grammarTransducers is the transducer part of every module whose
// interface carries one, read by direct field access as grammarParts reads
// the rest; a module whose interface carries none is absent.
func grammarTransducers() map[string]*structuralPart {
	chess := tabnaschess.Translate()
	csv := tabnascsv.Translate()
	ini := tabnasini.Translate()
	json := tabnasjson.Translate()
	json5 := tabnasjson5.Translate()
	jsonc := tabnasjsonc.Translate()
	jsonic := tabnasjsonic.Translate()
	jsonl := tabnasjsonl.Translate()
	markdown := tabnasmarkdown.Translate()
	toml := tabnastoml.Translate()
	xml := tabnasxml.Translate()
	yaml := tabnasyaml.Translate()
	zon := tabnaszon.Translate()
	return map[string]*structuralPart{
		"csv":    packagePart(csv.Transduce != nil, func() (string, string) { return csv.Transduce.Entry, csv.Transduce.Source }),
		"ini":    packagePart(ini.Transduce != nil, func() (string, string) { return ini.Transduce.Entry, ini.Transduce.Source }),
		"json":   packagePart(json.Transduce != nil, func() (string, string) { return json.Transduce.Entry, json.Transduce.Source }),
		"json5":  packagePart(json5.Transduce != nil, func() (string, string) { return json5.Transduce.Entry, json5.Transduce.Source }),
		"jsonc":  packagePart(jsonc.Transduce != nil, func() (string, string) { return jsonc.Transduce.Entry, jsonc.Transduce.Source }),
		"jsonic": packagePart(jsonic.Transduce != nil, func() (string, string) { return jsonic.Transduce.Entry, jsonic.Transduce.Source }),
		"jsonl":  packagePart(jsonl.Transduce != nil, func() (string, string) { return jsonl.Transduce.Entry, jsonl.Transduce.Source }),
		"markdown": packagePart(markdown.Transduce != nil, func() (string, string) {
			return markdown.Transduce.Entry, markdown.Transduce.Source
		}),
		"pgn":  packagePart(chess.Transduce != nil, func() (string, string) { return chess.Transduce.Entry, chess.Transduce.Source }),
		"toml": packagePart(toml.Transduce != nil, func() (string, string) { return toml.Transduce.Entry, toml.Transduce.Source }),
		"xml":  packagePart(xml.Transduce != nil, func() (string, string) { return xml.Transduce.Entry, xml.Transduce.Source }),
		"yaml": packagePart(yaml.Transduce != nil, func() (string, string) { return yaml.Transduce.Entry, yaml.Transduce.Source }),
		"zon":  packagePart(zon.Transduce != nil, func() (string, string) { return zon.Transduce.Entry, zon.Transduce.Source }),
	}
}

func localPart(entry, source string) *structuralPart {
	return &structuralPart{entry: entry, source: source}
}

func packagePart(present bool, fields func() (string, string)) *structuralPart {
	if !present {
		return nil
	}
	entry, source := fields()
	return localPart(entry, source)
}

// grammarParts adapts every package-local declaration by direct field access.
// A renamed or missing field therefore fails this test at compile time.
func grammarParts() map[string]structuralParts {
	csv := tabnascsv.Translate()
	ini := tabnasini.Translate()
	jsonGrammar := tabnasjson.Translate()
	json5 := tabnasjson5.Translate()
	jsonc := tabnasjsonc.Translate()
	jsonic := tabnasjsonic.Translate()
	jsonl := tabnasjsonl.Translate()
	markdown := tabnasmarkdown.Translate()
	toml := tabnastoml.Translate()
	xml := tabnasxml.Translate()
	yaml := tabnasyaml.Translate()
	zon := tabnaszon.Translate()
	return map[string]structuralParts{
		"csv": {
			csv.Manifest,
			packagePart(csv.Lift != nil, func() (string, string) { return csv.Lift.Entry, csv.Lift.Source }),
			packagePart(csv.Render != nil, func() (string, string) { return csv.Render.Entry, csv.Render.Source }),
		},
		"ini": {
			ini.Manifest,
			packagePart(ini.Lift != nil, func() (string, string) { return ini.Lift.Entry, ini.Lift.Source }),
			packagePart(ini.Render != nil, func() (string, string) { return ini.Render.Entry, ini.Render.Source }),
		},
		"json": {
			jsonGrammar.Manifest,
			packagePart(jsonGrammar.Lift != nil, func() (string, string) { return jsonGrammar.Lift.Entry, jsonGrammar.Lift.Source }),
			packagePart(jsonGrammar.Render != nil, func() (string, string) { return jsonGrammar.Render.Entry, jsonGrammar.Render.Source }),
		},
		"json5": {
			json5.Manifest,
			packagePart(json5.Lift != nil, func() (string, string) { return json5.Lift.Entry, json5.Lift.Source }),
			packagePart(json5.Render != nil, func() (string, string) { return json5.Render.Entry, json5.Render.Source }),
		},
		"jsonc": {
			jsonc.Manifest,
			packagePart(jsonc.Lift != nil, func() (string, string) { return jsonc.Lift.Entry, jsonc.Lift.Source }),
			packagePart(jsonc.Render != nil, func() (string, string) { return jsonc.Render.Entry, jsonc.Render.Source }),
		},
		"jsonic": {
			jsonic.Manifest,
			packagePart(jsonic.Lift != nil, func() (string, string) { return jsonic.Lift.Entry, jsonic.Lift.Source }),
			packagePart(jsonic.Render != nil, func() (string, string) { return jsonic.Render.Entry, jsonic.Render.Source }),
		},
		"jsonl": {
			jsonl.Manifest,
			packagePart(jsonl.Lift != nil, func() (string, string) { return jsonl.Lift.Entry, jsonl.Lift.Source }),
			packagePart(jsonl.Render != nil, func() (string, string) { return jsonl.Render.Entry, jsonl.Render.Source }),
		},
		"markdown": {
			markdown.Manifest,
			packagePart(markdown.Lift != nil, func() (string, string) { return markdown.Lift.Entry, markdown.Lift.Source }),
			packagePart(markdown.Render != nil, func() (string, string) { return markdown.Render.Entry, markdown.Render.Source }),
		},
		"toml": {
			toml.Manifest,
			packagePart(toml.Lift != nil, func() (string, string) { return toml.Lift.Entry, toml.Lift.Source }),
			packagePart(toml.Render != nil, func() (string, string) { return toml.Render.Entry, toml.Render.Source }),
		},
		"xml": {
			xml.Manifest,
			packagePart(xml.Lift != nil, func() (string, string) { return xml.Lift.Entry, xml.Lift.Source }),
			packagePart(xml.Render != nil, func() (string, string) { return xml.Render.Entry, xml.Render.Source }),
		},
		"yaml": {
			yaml.Manifest,
			packagePart(yaml.Lift != nil, func() (string, string) { return yaml.Lift.Entry, yaml.Lift.Source }),
			packagePart(yaml.Render != nil, func() (string, string) { return yaml.Render.Entry, yaml.Render.Source }),
		},
		"zon": {
			zon.Manifest,
			packagePart(zon.Lift != nil, func() (string, string) { return zon.Lift.Entry, zon.Lift.Source }),
			packagePart(zon.Render != nil, func() (string, string) { return zon.Render.Entry, zon.Render.Source }),
		},
	}
}

func declaredShapes(t *testing.T, format string, value any) []string {
	t.Helper()
	switch value := value.(type) {
	case string:
		return []string{value}
	case []any:
		out := make([]string, len(value))
		for i, item := range value {
			shape, ok := item.(string)
			if !ok {
				t.Fatalf("%s read shape %d is %#v", format, i, item)
			}
			out[i] = shape
		}
		return out
	default:
		t.Fatalf("%s reads is %#v", format, value)
		return nil
	}
}

func assertStructuralPart(t *testing.T, format, kind string, declared *string, part *structuralPart) {
	t.Helper()
	if declared == nil {
		if part != nil {
			t.Fatalf("%s exposes an undeclared %s", format, kind)
		}
		return
	}
	if part == nil || part.entry == "" {
		t.Fatalf("%s does not expose its declared %s with an explicit entry", format, kind)
	}
	if strings.HasSuffix(*declared, ".alc") {
		if part.source == "" {
			t.Fatalf("%s %s does not embed %s", format, kind, *declared)
		}
	} else if part.entry != *declared || part.source != "" {
		t.Fatalf("%s builtin %s is entry %q with source length %d", format, kind, part.entry, len(part.source))
	}
}

func grammarParser(t *testing.T, format string) *tabnas.Tabnas {
	t.Helper()
	switch format {
	case "csv":
		parser, err := tabnascsv.Make()
		if err != nil {
			t.Fatal(err)
		}
		return parser
	case "ini":
		return tabnasini.MakeJsonic()
	case "json":
		return tabnasjson.Make()
	case "json5":
		parser := tabnasjsonic.Make()
		if err := parser.UseDefaults(tabnasjson5.Json5, tabnasjson5.Defaults()); err != nil {
			t.Fatal(err)
		}
		return parser
	case "jsonc":
		parser := tabnasjsonic.Make()
		if err := parser.Use(tabnasjsonc.Jsonc); err != nil {
			t.Fatal(err)
		}
		return parser
	case "jsonic":
		return tabnasjsonic.Make()
	case "jsonl":
		return tabnasjsonl.Make()
	case "markdown":
		return tabnasmarkdown.Make()
	case "toml":
		return tabnastoml.MakeJsonic()
	case "xml":
		parser := tabnasjsonic.Make()
		if err := parser.Use(tabnasxml.Xml); err != nil {
			t.Fatal(err)
		}
		return parser
	case "yaml":
		return tabnasyaml.MakeJsonic()
	case "zon":
		return tabnaszon.MakeJsonic()
	default:
		t.Fatalf("no parser for %s", format)
		return nil
	}
}

func conformanceMain(parts structuralParts, reads, writes string, lifted bool) (string, error) {
	// The producer is structural fact: only the preferred shape may come
	// from a lift; every other shape is the grammar's raw tree. The adapter
	// is selected from the descriptor, so a false reads value type-fails.
	source := "events input"
	if lifted {
		source = fmt.Sprintf("%s (events input)", parts.lift.entry)
	}
	definitions := ""
	adapted := ""
	switch {
	case reads == writes:
		adapted = source
	case reads == "tree" && writes == "records":
		definitions = "def conformance-binding\n  record\n    entry :columns :infer\n" +
			"    entry :rows (path each-index)\n\n"
		adapted = fmt.Sprintf("table-from-json conformance-binding (%s)", source)
	case reads == "records" && writes == "tree":
		adapted = fmt.Sprintf("records (%s)", source)
	case writes == "text":
		definitions = "def conformance-text [input]\n" +
			"  join \"\"\n" +
			"    map\n" +
			"      fn [event]\n" +
			"        match event\n" +
			"          case (scalar value) (scalar-text csv-options value)\n" +
			"          case _ \"\"\n" +
			"      input\n\n"
		adapted = fmt.Sprintf("conformance-text (%s)", source)
	default:
		return "", fmt.Errorf("no %s-to-%s conformance adapter", reads, writes)
	}
	call := fmt.Sprintf("%s (%s)", parts.render.entry, adapted)
	if parts.render.entry == "csv" {
		call = fmt.Sprintf("csv csv-options (%s)", adapted)
	}
	return definitions + fmt.Sprintf("def export [input]\n  %s\n", call), nil
}

func conformanceEvents(t *testing.T, format, text string) []tt.Event {
	t.Helper()
	var recorder tt.Recorder
	if _, fail := tt.NewParserSource(grammarParser(t, format), text).Run(&recorder); fail != nil {
		t.Fatalf("%s does not parse %q: %v", format, text, fail)
	}
	return recorder.Events
}

func TestStructuralTranslationParts(t *testing.T) {
	shapes := map[string]bool{"text": true, "records": true, "tree": true}
	samples := map[string]string{
		"csv":      "a,b\n1,x\n2,y\n",
		"ini":      "a=1\nb=x\n",
		"json":     "{\"a\":1,\"b\":[true,null]}\n",
		"json5":    "{a:1,b:['x',true]}\n",
		"jsonc":    "{\"a\":1,/* c */\"b\":true}\n",
		"jsonic":   "a:1,b:true",
		"jsonl":    "{\"a\":1}\n{\"a\":2}\n",
		"markdown": "| a | b |\n| - | - |\n| 1 | x |\n",
		"toml":     "a = 1\nb = \"x\"\n",
		"xml":      "<a x=\"1\">text</a>",
		"yaml":     "a: 1\nb:\n  - x\n  - y\n",
		"zon":      ".{ .a = 1, .b = .{ true, false } }",
	}
	for format, parts := range grammarParts() {
		t.Run(format, func(t *testing.T) {
			var descriptor translationDescriptor
			if err := json.Unmarshal([]byte(parts.manifest), &descriptor); err != nil {
				t.Fatal(err)
			}
			if descriptor.LanguageID != format {
				t.Fatalf("languageId is %q", descriptor.LanguageID)
			}
			reads := declaredShapes(t, format, descriptor.Translate.Reads)
			if len(reads) == 0 {
				t.Fatal("no read shape")
			}
			for _, shape := range reads {
				if !shapes[shape] {
					t.Fatalf("unknown read shape %q", shape)
				}
			}
			if !shapes[descriptor.Translate.Writes] {
				t.Fatalf("unknown write shape %q", descriptor.Translate.Writes)
			}
			assertStructuralPart(t, format, "lift", descriptor.Translate.Lift, parts.lift)
			render := descriptor.Translate.Render
			assertStructuralPart(t, format, "render", &render, parts.render)

			// The transducer part, where the manifest names one: the module
			// exposes it under its entry, the command reads it from the
			// module (Format.Transducer), and it composes over the format's
			// parser's rule passes into a tree target. Where the manifest
			// names none, neither has one.
			transduce := grammarTransducers()[format]
			assertStructuralPart(t, format, "transduce", descriptor.Translate.Transduce, transduce)
			registered := translate.Named(format)
			if registered == nil {
				t.Fatalf("%s is one of the command's formats", format)
			}
			var exposed, read string
			if transduce != nil {
				exposed = transduce.entry
			}
			if registered.Transducer != nil {
				read = registered.Transducer.Entry
			}
			if read != exposed {
				t.Fatalf("the command's transducer is %q, the module's %q", read, exposed)
			}
			if registered.Transducer != nil && descriptor.Translate.Transduce != nil {
				if !strings.HasSuffix(registered.Transducer.File, *descriptor.Translate.Transduce) {
					t.Fatalf("the transducer is %s, not %s", registered.Transducer.File, *descriptor.Translate.Transduce)
				}
				composition, f := altranslate.ComposeParsed(registered.Part, registered.Transducer, translate.Named("json").Part, altranslate.DefaultOptions(), "parts")
				if f != nil {
					t.Fatalf("the transducer does not compose: %v", f)
				}
				if _, f := composition.Compile(nil, routers, renderers); f != nil {
					t.Fatalf("the transducer's composition does not compile: %v", f)
				}
			}

			var program *Program
			for index, readShape := range reads {
				main, err := conformanceMain(parts, readShape, descriptor.Translate.Writes, index == 0 && parts.lift != nil)
				if err != nil {
					t.Fatalf("%s reads %s: %v", format, readShape, err)
				}
				sources := []Source{{File: format + "/" + readShape + "-conformance.alc", Text: main}}
				for _, item := range []struct {
					path *string
					part *structuralPart
				}{
					{descriptor.Translate.Lift, parts.lift},
					{&render, parts.render},
				} {
					if item.part == nil || item.part.source == "" {
						continue
					}
					sources = append(sources, Source{File: *item.path, Text: item.part.source})
				}
				compiled, fail := CompileSources(sources, routers, renderers)
				if fail != nil {
					t.Fatalf("%s reads %s: %v", format, readShape, fail)
				}
				for kind, part := range map[string]*structuralPart{"lift": parts.lift, "render": parts.render} {
					if part != nil && part.source != "" && compiled.Resolved().Get(part.entry) == nil {
						t.Fatalf("%s source does not define %s", kind, part.entry)
					}
				}
				if index == 0 {
					program = compiled
				}
			}
			if program == nil {
				t.Fatal("no preferred read-shape program")
			}

			first := conformanceEvents(t, format, samples[format])
			rendered, runFail := replayEvents(program, first, RenderDefault, tt.DefaultLimits(), tt.NewMetrics())
			if runFail != nil {
				t.Fatalf("translation parts do not render: %v", runFail)
			}
			second := conformanceEvents(t, format, rendered)
			rerendered, runFail := replayEvents(program, second, RenderDefault, tt.DefaultLimits(), tt.NewMetrics())
			if runFail != nil {
				t.Fatalf("the second render fails: %v", runFail)
			}
			if rerendered != rendered {
				t.Fatalf("render is not round-trip stable:\nfirst:  %q\nsecond: %q", rendered, rerendered)
			}
		})
	}
}
