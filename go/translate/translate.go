// Copyright (c) 2026 tabnas, MIT License

// Package translate is `dmog translate`: a document in one of the
// formats whose grammar modules this command carries, written in any of
// them. It is the Go port of rs/src/translate.rs, and the command
// (cmd/dmog) and its tests both build on it.
//
// Every format's module exports its translation parts: its manifest's
// translate object (the shapes it reads and writes, the root its render
// needs, the schema its events carry), its render, and, where it has them,
// its lift and its embedding, each an alchemy file. This package reads them
// into alchemy's Parts, has alchemy's translate package compose the route
// (the source's lift, the root adapters, the embedding, the inferred table,
// the render), compiles the composition with transduce's routers and
// render's renderers, and runs it over the input read with the source
// format's grammar. A module that ships a transducer part (its manifest's
// translate.transduce, the alchemy file whose entry makes the format's JSON
// events of the parser's rule passes) is read through it: the route is
// composed over the transducer (at.ComposeParsed), and the input's rule
// passes stream through transduce's ParserEventSource into the program,
// whatever the grammar, with nothing held but what the part keeps. Any
// other format is parsed whole and its value walked, and so is a document
// when a path selects a value below the root, or when the request asks it
// read whole (--whole). The output is written as the run goes, so a
// document whose events are not what the grammar's value holds, which the
// part refuses part way (a YAML stream of several documents, a merge key, a
// repeated member, which a part streams twice and tree-contract refuses),
// fails with its code after what it wrote: --whole reads it whole, its
// grammar's value the document (admin ADR-33). Where a module's parser
// builds a value that is not yet the tree its parts declare (an expression's
// operators, proto's syntax tree, a typed feed or database, a grammar
// notation's document, which compiles to a grammar spec), the tree the
// module's API reads a document as is the document, read whole. Nothing
// here knows a format by its name: a format is what its manifest says, and
// a module whose manifest names no parts this host can take is not a
// format here.
//
// Neither route needed transduce's rule-event adapter (the tabnas_nodecell
// build tag), which transduce 0.5.0 retired, so every build reads a document
// as every other does.
package translate

import (
	"encoding/json"
	"fmt"
	"io"
	"math"
	"math/big"
	"sort"
	"strconv"
	"strings"
	"sync"
	"unicode"

	tabnasabnf "github.com/tabnas/abnf/go"
	alchemy "github.com/tabnas/alchemy/go"
	at "github.com/tabnas/alchemy/go/translate"
	tabnasc "github.com/tabnas/c/go"
	tabnaschess "github.com/tabnas/chess/go"
	tabnascss "github.com/tabnas/css/go"
	tabnascsv "github.com/tabnas/csv/go"
	tabnasebnf "github.com/tabnas/ebnf/go"
	tabnasexpr "github.com/tabnas/expr/go"
	tabnasfeed "github.com/tabnas/feed/go"
	tabnasgbnf "github.com/tabnas/gbnf/go"
	tabnasini "github.com/tabnas/ini/go"
	tabnasjson "github.com/tabnas/json/go"
	tabnasjson5 "github.com/tabnas/json5/go"
	tabnasjsonc "github.com/tabnas/jsonc/go"
	tabnasjsonic "github.com/tabnas/jsonic/go"
	tabnasjsonl "github.com/tabnas/jsonl/go"
	tabnasmarkdown "github.com/tabnas/markdown/go"
	tabnas "github.com/tabnas/parser/go"
	tabnasproto "github.com/tabnas/proto/go"
	tr "github.com/tabnas/render/go"
	tabnassemver "github.com/tabnas/semver/go"
	tabnastoml "github.com/tabnas/toml/go"
	tt "github.com/tabnas/transduce/go"
	tabnasxml "github.com/tabnas/xml/go"
	tabnasyaml "github.com/tabnas/yaml/go"
	tabnaszon "github.com/tabnas/zon/go"
)

// Format is one format this command reads and writes: its parts, as its
// module's manifest names them, and the grammar its documents are read
// with.
type Format struct {
	Part *at.Part
	// reader is a source over a text with a fresh parser of the format's
	// grammar under its own options: a ParserSource takes its parser over,
	// so each read has one. It is nil where the module has no parser here.
	reader func(text string) *tt.ParserSource
	// passes is a source of the parser's rule passes over a text, with a
	// fresh parser as reader has one: what a program over parser events
	// reads (a transducer part, run --grammar). It is nil where the module
	// has no parser here.
	passes func(text string) *tt.ParserEventSource
	// tree is a document as the tree the format's module reads it as with
	// its own API, where its parser's value is not yet the tree its parts
	// declare (an expression's operators, or a typed value): a whole read
	// reads it. It is nil where reader is not, but for a module that ships
	// a transducer part (PGN): its parser's rule passes stream through the
	// part, and a document the part does not stream is read whole as this
	// tree.
	tree func(text string) (any, *tt.Fail)
	// check is what the module's own parse checks before its parser runs,
	// which the parser does not check itself: a document it refuses is not
	// parsed. Nil where the module checks nothing first.
	check func(text string) *tt.Fail
	// Transducer is the transducer part the format's module ships, its
	// manifest's translate.transduce: the alchemy file whose entry makes
	// the format's JSON events of its parser's rule passes, through which
	// translate reads the format's documents. Nil where the module ships
	// none, and the documents are read whole.
	Transducer *at.Alc
	// compileOnce compiles Transducer on its own, once asked for
	// (TransducerProgram), into compiled or compileFail.
	compileOnce sync.Once
	compiled    *alchemy.Program
	compileFail *tt.Fail
}

// ID is the manifest's languageId.
func (f *Format) ID() string { return f.Part.ID }

// Parses is whether the format's documents are parsed, so that a program
// over parser events can read them (Passes); a format read only as the
// tree its module's API makes of a document has no parser here.
func (f *Format) Parses() bool { return f.passes != nil }

// Passes is the parser's rule passes over text: transduce's source of
// ParserEvents/1 with a fresh parser of the format's grammar, for a
// program over parser events, a transducer part's or run --grammar's. The
// check the module's parse makes first, where it makes one, runs first,
// and its refusal is the failure; a format read only as a tree has no
// parser, and refuses.
func (f *Format) Passes(text string) (*tt.ParserEventSource, *tt.Fail) {
	if f.passes == nil {
		return nil, tt.InputFail(f.ID() + " is read as a tree, not parsed: it has no rule passes for a program to read")
	}
	if f.check != nil {
		if fail := f.check(text); fail != nil {
			return nil, fail
		}
	}
	return f.passes(text), nil
}

// run drives sink with a document's events as the format reads it whole,
// after the check its module's own parse makes first, where it makes one:
// its parser's value, walked by transduce's ParserSource, or the tree its
// module's API reads, walked as the ParserSource walks a parser's value,
// under the same limits and counts.
func (f *Format) run(input string, limits tt.Limits, metrics *tt.Metrics, sink tt.Sink) (tt.Flow, *tt.Fail) {
	if f.check != nil {
		if fail := f.check(input); fail != nil {
			return tt.Continue, fail
		}
	}
	if f.tree == nil {
		return f.reader(input).Limits(limits).Metrics(metrics).Run(sink)
	}
	guarded := tt.NewGuarded(sink, limits, tt.NewAbortFlag(), metrics)
	defer guarded.Flush()
	tree, fail := f.tree(input)
	if fail != nil {
		return tt.Continue, fail
	}
	flow, fail := tt.WalkValue(tree, guarded)
	if fail != nil || flow == tt.Stop {
		return flow, fail
	}
	return guarded.Event(tt.EvEnd())
}

// TransducerProgram is the transducer part compiled on its own, over the
// parser's rule passes, its entry the export: the stage a host puts in
// front of a program of its own, as run does in front of a program over a
// JSON document, and the one Read reads through. It is compiled once, when
// first asked for, and nil where the module ships no part.
func (f *Format) TransducerProgram() (*alchemy.Program, *tt.Fail) {
	f.compileOnce.Do(func() {
		alc := f.Transducer
		if alc == nil {
			return
		}
		sources := []alchemy.Source{
			{File: alc.File, Text: alc.Text},
			{File: "main", Text: "def export [input] (" + alc.Entry + " input)"},
		}
		f.compiled, f.compileFail = alchemy.CompileSourcesWith(sources, alchemy.InputParserEvents, tt.Routers(), tr.Renderers())
	})
	return f.compiled, f.compileFail
}

// Read is a document read whole with this format's grammar, as a value:
// its events as translate reads them, through the transducer part its
// module ships (so that a number keeps the lexeme the document spelled it
// with), collected; or the grammar's own value, walked, where the module
// ships no part and where the part's events are not what the grammar's
// value holds (a repeated member, which the part streams twice). A
// repeated member keeps its last value.
func (f *Format) Read(input string, limits tt.Limits) (tt.Datum, *tt.Fail) {
	part, fail := f.TransducerProgram()
	if fail != nil {
		return tt.Datum{}, fail
	}
	if part != nil {
		value, fail := readThrough(f, part, input, limits)
		if fail == nil || !unfollowed(fail) {
			return value, fail
		}
	}
	return read(f, input, limits)
}

// modulePart is a grammar module's TranslationPart. Every module owns its
// interface types, which have this shape, so a module's part converts to
// it field for field, and a module whose part changes shape fails to
// compile here.
type modulePart struct {
	Entry  string
	Source string
}

// partText is a module's part as alchemy's: nil for none, a render alchemy
// carries for a part with no source (json, csv), an alchemy file
// otherwise.
func partText(p *modulePart) *at.PartText {
	switch {
	case p == nil:
		return nil
	case p.Source == "":
		return at.CarriedPartText(p.Entry)
	}
	return at.NewPartText(p.Entry, p.Source)
}

// descriptor is a module's parts as alchemy's descriptor, named by the
// module's package name in diagnostics.
func descriptor(name, manifest string, lift, embed, render *modulePart) at.Descriptor {
	return at.Descriptor{
		Package:  name,
		Manifest: manifest,
		Lift:     partText(lift),
		Embed:    partText(embed),
		Render:   partText(render),
	}
}

// installed panics on the error a grammar module's plugin gives when it
// does not install: its documents are fixed, so only a defect of the
// module fails it, as the modules' own constructors panic.
func installed(name string, err error) {
	if err != nil {
		panic(name + ": the grammar does not install: " + err.Error())
	}
}

// module is a grammar module this command carries: its parts, the
// transducer part it hands over beside them where its interface carries
// one (nil otherwise; PartTransducer reads the two together), its parser,
// a fresh one each time, or its tree, or both where a transducer part
// reads the parser's passes and a whole read the tree, and its check
// (Format's).
type module struct {
	descriptor func() at.Descriptor
	transduce  func() *at.PartText
	parser     func() *tabnas.Tabnas
	tree       func(text string) (any, *tt.Fail)
	check      func(text string) *tt.Fail
}

// modules is every grammar module this command carries, each with how its
// documents are read: the module's own parser, under its own options, or,
// where its parser's value is not yet the tree its parts declare, the tree
// its own API reads a document as. chess's is both: a database streams
// through its transducer part over the parser's rule passes, and one the
// part does not stream is read whole as the typed database's tree.
func modules() []module {
	return []module{
		{descriptor: func() at.Descriptor {
			p := tabnasabnf.Translate()
			return descriptor("tabnas-abnf", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: abnfTree},
		{descriptor: func() at.Descriptor {
			p := tabnasc.Translate()
			return descriptor("tabnas-c", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, parser: func() *tabnas.Tabnas {
			parser, err := tabnasc.MakeC()
			installed("tabnas-c", err)
			return parser
		}},
		{descriptor: func() at.Descriptor {
			p := tabnaschess.Translate()
			return descriptor("tabnas-chess", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnaschess.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnaschess.Make() }, tree: pgnTree},
		{descriptor: func() at.Descriptor {
			p := tabnascss.Translate()
			return descriptor("tabnas-css", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnascss.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnascss.MakeJsonic() }},
		{descriptor: func() at.Descriptor {
			p := tabnascsv.Translate()
			return descriptor("tabnas-csv", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnascsv.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas {
			parser, err := tabnascsv.Make()
			installed("tabnas-csv", err)
			return parser
		}},
		{descriptor: func() at.Descriptor {
			p := tabnasebnf.Translate()
			return descriptor("tabnas-ebnf", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: ebnfTree},
		{descriptor: func() at.Descriptor {
			p := tabnasexpr.Translate()
			return descriptor("tabnas-expr", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: exprTree},
		{descriptor: func() at.Descriptor {
			p := tabnasfeed.Translate()
			return descriptor("tabnas-feed", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: feedTree},
		{descriptor: func() at.Descriptor {
			p := tabnasgbnf.Translate()
			return descriptor("tabnas-gbnf", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: gbnfTree},
		{descriptor: func() at.Descriptor {
			p := tabnasini.Translate()
			return descriptor("tabnas-ini", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasini.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnasini.MakeJsonic() }},
		{descriptor: func() at.Descriptor {
			p := tabnasjson.Translate()
			return descriptor("tabnas-json", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasjson.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnasjson.Make() }},
		{descriptor: func() at.Descriptor {
			p := tabnasjson5.Translate()
			return descriptor("tabnas-json5", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasjson5.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas {
			return json5Parser()
		}, check: json5Check},
		{descriptor: func() at.Descriptor {
			p := tabnasjsonc.Translate()
			return descriptor("tabnas-jsonc", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasjsonc.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas {
			parser := tabnasjsonic.Make()
			installed("tabnas-jsonc", parser.Use(tabnasjsonc.Jsonc))
			return parser
		}},
		{descriptor: func() at.Descriptor {
			p := tabnasjsonic.Translate()
			return descriptor("tabnas-jsonic", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasjsonic.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnasjsonic.Make() }},
		{descriptor: func() at.Descriptor {
			p := tabnasjsonl.Translate()
			return descriptor("tabnas-jsonl", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasjsonl.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnasjsonl.Make() }},
		{descriptor: func() at.Descriptor {
			p := tabnasmarkdown.Translate()
			return descriptor("tabnas-markdown", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, parser: func() *tabnas.Tabnas { return tabnasmarkdown.Make() }},
		{descriptor: func() at.Descriptor {
			p := tabnasproto.Translate()
			return descriptor("tabnas-proto", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, tree: protoTree},
		{descriptor: func() at.Descriptor {
			p := tabnassemver.Translate()
			return descriptor("tabnas-semver", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, parser: func() *tabnas.Tabnas { return tabnassemver.Make() }},
		{descriptor: func() at.Descriptor {
			p := tabnastoml.Translate()
			return descriptor("tabnas-toml", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnastoml.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnastoml.MakeJsonic() }},
		{descriptor: func() at.Descriptor {
			p := tabnasxml.Translate()
			return descriptor("tabnas-xml", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasxml.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas {
			// XML's plugin, outside its embed mode, reconfigures the jsonic
			// host it is installed on as a pure XML parser, jsonic's own
			// grammar and lexers unreachable.
			parser := tabnasjsonic.Make()
			installed("tabnas-xml", parser.UseDefaults(tabnasxml.Xml, tabnasxml.Defaults))
			return parser
		}},
		{descriptor: func() at.Descriptor {
			p := tabnasyaml.Translate()
			return descriptor("tabnas-yaml", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnasyaml.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return tabnasyaml.MakeJsonic() }},
		{descriptor: func() at.Descriptor {
			p := tabnaszon.Translate()
			return descriptor("tabnas-zon", p.Manifest, (*modulePart)(p.Lift), (*modulePart)(p.Embed), (*modulePart)(p.Render))
		}, transduce: func() *at.PartText {
			return partText((*modulePart)(tabnaszon.Translate().Transduce))
		}, parser: func() *tabnas.Tabnas { return zonParser() }},
	}
}

// bigKey is the member of the object that is ZON's reader's big integer.
const bigKey = "$big"

// zonParser is ZON's parser, with an integer no float64 holds exactly as
// the object {"$big": "<digits>"}, the digits after a minus sign when it is
// negative: the reader's big integer as ZON's translation part declares it
// and its Rust reader builds it. The Go reader builds a *big.Int, which no
// transduce event carries, so the token's value is replaced as it is read,
// before a rule takes it, and the value and the events hold the object.
func zonParser() *tabnas.Tabnas {
	parser := tabnaszon.MakeJsonic()
	parser.Sub(func(tkn *tabnas.Token, _ *tabnas.Rule, _ *tabnas.Context) {
		if n, ok := tkn.Val.(*big.Int); ok {
			tkn.Val = &tabnas.OrderedMap{Keys: []string{bigKey}, Vals: map[string]any{bigKey: n.String()}}
		}
	}, nil)
	return parser
}

// A tree reader's parser is built once and reused, as the modules' own
// one-call parses reuse theirs: building the grammar dominates a parse, a
// parse builds a fresh context and only reads the instance, and no source
// installs a budget or a subscriber on it.
var feedParser = sync.OnceValue(func() *tabnas.Tabnas {
	parser := tabnas.Make()
	installed("tabnas-feed", parser.UseDefaults(tabnasfeed.Feed, tabnasfeed.Defaults))
	return parser
})

// json5Parser is json5's parser with its default options, on jsonic's.
func json5Parser() *tabnas.Tabnas {
	parser := tabnasjsonic.Make()
	installed("tabnas-json5", parser.UseDefaults(tabnasjson5.Json5, tabnasjson5.Defaults()))
	return parser
}

// json5Check is what json5's Parse checks before its parser runs, which
// the parser does not check itself: a document holding no value is refused
// as json5_empty or json5_no_value, rather than as the engine's unexpected.
// Only a document that may hold none, one that is empty or opens, after
// whitespace, with a comment, is put to the module's Parse.
func json5Check(text string) *tt.Fail {
	rest := strings.TrimLeftFunc(text, func(r rune) bool { return unicode.IsSpace(r) || r == '\ufeff' })
	if rest != "" && !strings.HasPrefix(rest, "/") {
		return nil
	}
	_, err := tabnasjson5.Parse(json5Parser(), text)
	if te, ok := err.(*tabnas.TabnasError); ok && (te.Code == "json5_empty" || te.Code == "json5_no_value") {
		return engineFailure(te)
	}
	return nil
}

// exprTree is an expression as expr's module reads one for its shared
// fixtures, the tree its parts declare: each operation a list whose first
// element is the operator's source text, and each object's members in the
// order the parse gives them (SimplifyOrdered of its Parse). The parser's
// own value holds an operation's operator as an *Op, which no transduce
// event carries.
func exprTree(text string) (any, *tt.Fail) {
	value, err := tabnasexpr.Parse(text)
	if err != nil {
		return nil, engineFailure(err)
	}
	return tabnasexpr.SimplifyOrdered(value), nil
}

// protoTree is a .proto file as proto's module reads one, the descriptor
// its parts declare as a tree, every member named and ordered as the
// canonical parse gives it (ParseValue). Its parse makes the check the
// parser does not, before the engine runs: a document nesting deeper than
// its cap is refused before the engine builds a tree that deep. The
// parser's own value is the grammar's syntax tree.
func protoTree(text string) (any, *tt.Fail) {
	tree, err := tabnasproto.ParseValue(text, nil)
	if err != nil {
		return nil, engineFailure(err)
	}
	return tree, nil
}

// abnfTree is an ABNF document as abnf's module reads one for a host, the
// tree its parts declare: the pure-data grammar spec its compiler writes
// (AbnfCompile, recognition off, so that the tree builders stay, as strict
// JSON), read as JSON. The grammar spec is the schema grammar-spec, which
// the EBNF and GBNF compilers emit too. Go's serializer writes its members
// in the order the compiler emits them, as TypeScript's and Rust's do, from
// tabnas-bnf 0.1.30; before, it wrote them in name order, and the match
// tokens' order in a list of its own (options.match.tokenOrder).
func abnfTree(text string) (any, *tt.Fail) {
	recognition := false
	compiled, err := tabnasabnf.AbnfCompile(text, &tabnasabnf.AbnfCompileOptions{Recognition: &recognition, Strict: true})
	if err != nil {
		return nil, tt.InputFail(err.Error())
	}
	return jsonTree(compiled)
}

// compiledTree is a notation's conversion, converted with the builtins on,
// as the shared compiler writes it for a host: the pure-data grammar spec,
// with its tree builders (ToJsonic over ToPureSpec, recognition off), as
// strict JSON, read as JSON. The compiler is tabnas-bnf, whose serializers
// abnf's module aliases, so reading a notation takes no module beyond the
// notations'.
func compiledTree(spec *tabnas.GrammarSpec, err error) (any, *tt.Fail) {
	if err != nil {
		return nil, tt.InputFail(err.Error())
	}
	pure, err := tabnasabnf.ToPureSpec(spec)
	if err != nil {
		return nil, tt.InputFail(err.Error())
	}
	return jsonTree(tabnasabnf.ToJsonic(pure, true, 0))
}

// ebnfTree is an EBNF document as ebnf's module reads one for a host, the
// tree its parts declare: the grammar spec the shared compiler writes of
// its conversion with the builtins on (compiledTree).
func ebnfTree(text string) (any, *tt.Fail) {
	return compiledTree(tabnasebnf.Ebnf(text, &tabnasebnf.ConvertOptions{Builtins: true}))
}

// gbnfTree is a GBNF document as gbnf's module reads one for a host, the
// tree its parts declare: the grammar spec the shared compiler writes of
// its conversion with the builtins on, and the notation's own defaults
// otherwise (the start root, eager classes where they are unambiguous)
// (compiledTree).
func gbnfTree(text string) (any, *tt.Fail) {
	return compiledTree(tabnasgbnf.Gbnf(text, &tabnasgbnf.ConvertOptions{Builtins: true}))
}

// feedTree is a feed as feed's module reads one by default, the Atom-shaped
// feed its parts declare: the parser builds it as a typed AtomFeed.
func feedTree(text string) (any, *tt.Fail) {
	feed, err := feedParser().Parse(text)
	if err != nil {
		return nil, engineFailure(err)
	}
	return plainTree(feed)
}

// pgnTree is a PGN database as chess's module reads one (its Parse), the
// tree its parts declare: the parser builds it as a typed Database.
func pgnTree(text string) (any, *tt.Fail) {
	database, err := tabnaschess.Parse(text)
	if err != nil {
		return nil, engineFailure(err)
	}
	return plainTree(database)
}

// plainTree is a module's typed value as the tree its JSON encoding names:
// encoding/json writes it, its fields' tags naming the members, in the
// fields' order (a map's in sorted key order), and the engine's ordered map
// reads it back, so that the walk keeps that order.
func plainTree(value any) (any, *tt.Fail) {
	text, err := json.Marshal(value)
	if err != nil {
		return nil, tt.InputFail("the document's value has no tree: " + err.Error())
	}
	return jsonTree(string(text))
}

// jsonTree is a JSON text as the tree it holds, its members in the text's
// order: the engine's ordered map reads it.
func jsonTree(text string) (any, *tt.Fail) {
	var tree tabnas.OrderedMap
	if err := json.Unmarshal([]byte(`{"tree":`+text+`}`), &tree); err != nil {
		return nil, tt.InputFail("the document's value has no tree: " + err.Error())
	}
	return tree.Vals["tree"], nil
}

// engineFailure is an engine error as transduce's ParserSource reports it:
// the input's, with the engine's code and position, and a cancel, which no
// abort of this command's asks for, a guard of the grammar's own.
func engineFailure(err error) *tt.Fail {
	te, ok := err.(*tabnas.TabnasError)
	if !ok {
		return tt.InputFail(err.Error())
	}
	f := tt.FailFromTabnas(te)
	if te.Code == "cancel" {
		f.Message = fmt.Sprintf("the grammar stopped the parse with a guard of its own (%s: %s); a grammar "+
			"may refuse nesting or size below this crate's Limits", te.Code, strings.TrimRight(te.Detail, " \t\r\n"))
	}
	return f
}

var (
	registryOnce sync.Once
	registry     []*Format
)

// Formats is every format, by id, read once. The slice is the caller's.
func Formats() []*Format {
	registryOnce.Do(func() {
		for _, m := range modules() {
			d := m.descriptor()
			part := at.PartFromDescriptor(d)
			if part == nil {
				continue
			}
			f := &Format{Part: part, tree: m.tree, check: m.check}
			if m.parser != nil {
				parser := m.parser
				f.reader = func(text string) *tt.ParserSource { return tt.NewParserSource(parser(), text) }
				f.passes = func(text string) *tt.ParserEventSource { return tt.NewParserEventSource(parser(), text) }
			}
			// A transducer the manifest and the module disagree on is none
			// here; the translation-parts test holds the two together.
			var transduce *at.PartText
			if m.transduce != nil {
				transduce = m.transduce()
			}
			if alc, ok := at.PartTransducer(d, transduce); ok {
				f.Transducer = alc
			}
			registry = append(registry, f)
		}
		sort.SliceStable(registry, func(i, j int) bool { return registry[i].Part.ID < registry[j].Part.ID })
	})
	return append([]*Format(nil), registry...)
}

// Named is the format a command line names, by its id, or nil.
func Named(id string) *Format {
	for _, f := range Formats() {
		if f.ID() == id {
			return f
		}
	}
	return nil
}

// Names is the formats' ids for a message: `csv, ini, ... or zon`.
func Names() string { return namesOf(Formats()) }

// ParserNames is the ids of the formats whose documents are parsed, for a
// message, as Names lists every format: what run --grammar takes.
func ParserNames() string {
	var parsed []*Format
	for _, f := range Formats() {
		if f.Parses() {
			parsed = append(parsed, f)
		}
	}
	return namesOf(parsed)
}

func namesOf(formats []*Format) string {
	ids := make([]string, len(formats))
	for i, f := range formats {
		ids[i] = f.ID()
	}
	switch len(ids) {
	case 0:
		return ""
	case 1:
		return ids[0]
	}
	return strings.Join(ids[:len(ids)-1], ", ") + " or " + ids[len(ids)-1]
}

// Segment is one step of a Path: a member's key, or an element's index.
type Segment struct {
	Key     string
	Index   uint64
	IsIndex bool
}

// Path is a value below a document's root, as the segments --path names.
type Path struct {
	Segments []Segment
}

// String is the path as the JSON array it was given as.
func (p *Path) String() string {
	items := make([]tt.Datum, len(p.Segments))
	for i, s := range p.Segments {
		if s.IsIndex {
			items[i] = tt.NumberDatumLexeme(float64(s.Index), strconv.FormatUint(s.Index, 10))
		} else {
			items[i] = tt.StringDatum(s.Key)
		}
	}
	return tt.ArrayDatum(items...).String()
}

// find is the value at the path below d.
func (p *Path) find(d *tt.Datum) (*tt.Datum, bool) {
	here := d
	for _, s := range p.Segments {
		switch {
		case s.IsIndex && here.Kind == tt.DatumArray:
			if s.Index >= uint64(len(here.Items)) {
				return nil, false
			}
			here = &here.Items[s.Index]
		case !s.IsIndex && here.Kind == tt.DatumObject:
			next, ok := here.Get(s.Key)
			if !ok {
				return nil, false
			}
			here = next
		default:
			return nil, false
		}
	}
	return here, true
}

// Request is what a translation was asked.
type Request struct {
	// From is the format the input is read with.
	From *Format
	// To is the format it is written in.
	To *Format
	// Path is a value below the root to translate instead of the whole
	// document, nil for the whole document: a plain tree, whatever the
	// source's shapes.
	Path *Path
	// Options are what the host chooses: the key a root is wrapped under.
	Options at.Options
	// Program is a program, as its file name and its text, whose export
	// stands in the source's place: its JSON events a tree, its table
	// records. Nil for none.
	Program *alchemy.Source
	// Whole is whether the document is read whole, as its grammar's value,
	// rather than streamed through its format's transducer part (--whole):
	// the route for a document the part refuses part way.
	Whole  bool
	Limits tt.Limits
}

// Compiled is the composition a request makes, and the program it compiles
// to: one compiled composition serves many inputs. It is the composition
// over the source's events, the route every format has, and, where the
// source ships a transducer part and the whole document is asked, not read
// whole, the composition over its parser's rule passes, which RunCompiled
// then runs in its place.
type Compiled struct {
	Composition *at.Composition
	Program     *alchemy.Program
	parsed      *parsedRoute
}

// parsedRoute is the composition over the source's parser's rule passes,
// through its transducer part, and its program.
type parsedRoute struct {
	composition *at.Composition
	program     *alchemy.Program
}

// Compile is the composition a request makes, and the program it compiles
// to.
func Compile(r *Request) (*Compiled, *tt.Fail) {
	routers, renderers := tt.Routers(), tr.Renderers()
	// The transducer the route reads the document through, where the
	// source ships one and the whole document is asked, not read whole: a
	// path selects a value below the root, which only the document's value
	// has.
	transducer := r.From.Transducer
	if r.Path != nil || r.Whole {
		transducer = nil
	}
	if r.Program != nil {
		file, text := r.Program.File, r.Program.Text
		program, f := alchemy.Compile(text, file, routers, renderers)
		if f != nil {
			return nil, f
		}
		output := program.Output()
		if output == alchemy.OutputText {
			return nil, tt.NewFail(tt.CodeDSLTypeError, "bad_output: "+file+"'s export writes its own text, and "+
				"translate takes a program whose export answers JSON events or a table, which a format's "+
				"render then writes")
		}
		composition, f := at.ComposeProgram(output, r.To.Part, r.Options, "translate")
		if f != nil {
			return nil, f
		}
		compiled, f := composition.Compile(&alchemy.Source{File: file, Text: text}, routers, renderers)
		if f != nil {
			return nil, f
		}
		result := &Compiled{Composition: composition, Program: compiled}
		if transducer != nil {
			over, f := at.ComposeProgramParsed(output, transducer, r.To.Part, r.Options, "translate")
			if f != nil {
				return nil, f
			}
			program, f := over.Compile(&alchemy.Source{File: file, Text: text}, routers, renderers)
			if f != nil {
				return nil, f
			}
			result.parsed = &parsedRoute{composition: over, program: program}
		}
		return result, nil
	}
	source := r.From.Part
	if r.Path != nil {
		source = nil
	}
	composition, f := at.Compose(source, r.To.Part, r.Options, "translate")
	if f != nil {
		return nil, f
	}
	compiled, f := composition.Compile(nil, routers, renderers)
	if f != nil {
		return nil, f
	}
	result := &Compiled{Composition: composition, Program: compiled}
	if transducer != nil {
		over, f := at.ComposeParsed(r.From.Part, transducer, r.To.Part, r.Options, "translate")
		if f != nil {
			return nil, f
		}
		program, f := over.Compile(nil, routers, renderers)
		if f != nil {
			return nil, f
		}
		result.parsed = &parsedRoute{composition: over, program: program}
	}
	return result, nil
}

// unfollowed is whether a transducer part's route refused a document
// because the grammar's value is not what the passes showed: the part's own
// refusal (fail :unstreamable), a second root, or a repeated member, which
// the part streams twice and the composition's policy refuses, since the
// grammar's value may keep the last or merge the two. The output streams as
// the run goes, so such a document is not read again: it fails with its
// code, and --whole (the request's Whole) reads it whole, as its grammar's
// value (admin ADR-33).
func unfollowed(f *tt.Fail) bool {
	return f.Code == tt.CodeStreamabilityUnknown || f.Code == tt.CodeDuplicateMember
}

// wholeHint is what a refusal of the transducer part's route adds to its
// message, where no program stands in the source's place, whose own events
// could make such a refusal: the route that reads the document.
const wholeHint = "; --whole reads the document whole"

// Run runs a request over input, writing the document to out as the run
// goes; metrics (a fresh set when nil) collects what the stages report, the
// output's bytes among it.
func Run(r *Request, input string, out io.Writer, metrics *tt.Metrics) *tt.Fail {
	compiled, f := Compile(r)
	if f != nil {
		return f
	}
	return RunCompiled(r, compiled, input, out, metrics)
}

// RunCompiled is Run, with the request's composition compiled already
// (Compile), so that one compiled composition serves many inputs. A failure
// after bytes have left says so (CommittedOutput): the source's failure
// (bad input after the rows) knows nothing of the output, and the writer's
// count does.
func RunCompiled(r *Request, compiled *Compiled, input string, out io.Writer, metrics *tt.Metrics) *tt.Fail {
	if metrics == nil {
		metrics = tt.NewMetrics()
	}
	f := runOnce(r, compiled, input, out, metrics)
	if f != nil && metrics.OutputBytes.Load() > 0 && !f.CommittedOutput {
		f.Committed()
	}
	return f
}

// runOnce is RunCompiled's one run: through the transducer part where the
// source ships one and the whole document is asked, and over the
// document's value otherwise.
func runOnce(r *Request, compiled *Compiled, input string, out io.Writer, metrics *tt.Metrics) *tt.Fail {
	if compiled.parsed != nil {
		// The transducer's route: the document's rule passes through the
		// part into the program, streamed whatever the grammar.
		sink, f := compiled.parsed.program.ParserSink(out, alchemy.RenderDefault, r.Limits, metrics)
		if f != nil {
			return f
		}
		source, f := r.From.Passes(input)
		if f != nil {
			return f
		}
		if _, f := source.Limits(r.Limits).Metrics(metrics).Run(sink); f != nil {
			if unfollowed(f) && r.Program == nil {
				f.Message += wholeHint
			}
			return f
		}
		return nil
	}
	sink, f := compiled.Program.Sink(out, alchemy.RenderDefault, r.Limits, metrics)
	if f != nil {
		return f
	}
	// The source's events reach a tree's render as the source made them, so
	// a stream that is no tree's is refused in front of it rather than
	// written half way.
	if compiled.Composition.Front == at.FrontTree {
		sink = tt.NewTreeContract(sink)
	}
	if r.Path == nil {
		_, f := r.From.run(input, r.Limits, metrics, sink)
		return f
	}
	value, f := read(r.From, input, r.Limits)
	if f != nil {
		return f
	}
	selected, ok := r.Path.find(&value)
	if !ok {
		return tt.InputFail("the path " + r.Path.String() + " names nothing in the document")
	}
	flow, f := tt.WalkDatum(selected, sink)
	if f != nil {
		return f
	}
	if flow == tt.Continue {
		if _, f := sink.Event(tt.EvEnd()); f != nil {
			return f
		}
	}
	return nil
}

// collect is the events of a document, collected into a value, which
// keeps the last of a repeated member.
type collect struct{ builder *tt.DatumBuilder }

func newCollect() *collect {
	return &collect{builder: tt.NewDatumBuilder(math.MaxInt, "max_capture_bytes", tt.LastWins)}
}

func (c *collect) Event(ev tt.Event) (tt.Flow, *tt.Fail) {
	if ev.Kind != tt.End {
		if f := c.builder.Event(ev); f != nil {
			return tt.Continue, f
		}
	}
	return tt.Continue, nil
}

// value is the value collected, once the document has ended.
func (c *collect) value() (tt.Datum, *tt.Fail) {
	value, ok := c.builder.Take()
	if !ok {
		return tt.Datum{}, tt.InputFail("the document holds no value")
	}
	return value, nil
}

// read is a document read whole with a format's grammar, as a value. A
// repeated member keeps its last value, as the grammar's own value does.
func read(format *Format, input string, limits tt.Limits) (tt.Datum, *tt.Fail) {
	c := newCollect()
	if _, f := format.run(input, limits, tt.NewMetrics(), c); f != nil {
		return tt.Datum{}, f
	}
	return c.value()
}

// readThrough is a document read through a format's transducer part, as a
// value: the part's events over the parser's rule passes, held to a tree's
// contract (one root, each key once in an object) and collected.
func readThrough(format *Format, part *alchemy.Program, input string, limits tt.Limits) (tt.Datum, *tt.Fail) {
	c := newCollect()
	metrics := tt.NewMetrics()
	head, f := part.ParserSinkInto(tt.NewTreeContract(c), limits, metrics)
	if f != nil {
		return tt.Datum{}, f
	}
	source, f := format.Passes(input)
	if f != nil {
		return tt.Datum{}, f
	}
	if _, f := source.Limits(limits).Metrics(metrics).Run(head); f != nil {
		return tt.Datum{}, f
	}
	return c.value()
}

// maxIndex is the largest index a path can name, 2^64 - 1, and twoTo64 the
// first whole number past it, as a float64.
const (
	maxIndex = math.MaxUint64
	twoTo64  = 18446744073709551616.0
)

// ParsePath is a path given as a JSON array of segments, each a string (a
// member's key) or a whole number (an element's index), the form alchemy's
// as-path validates.
func ParsePath(text string, limits tt.Limits) (*Path, *tt.Fail) {
	json := Named("json")
	if json == nil {
		return nil, tt.InputFail("no JSON grammar to read the path")
	}
	refuse := func() *tt.Fail {
		return tt.InputFail("--path takes a JSON array of keys and indexes, such as [\"people\",0], not " + text)
	}
	value, f := json.Read(text, limits)
	if f != nil || value.Kind != tt.DatumArray {
		return nil, refuse()
	}
	path := &Path{Segments: make([]Segment, 0, len(value.Items))}
	for _, item := range value.Items {
		switch {
		case item.Kind == tt.DatumString:
			path.Segments = append(path.Segments, Segment{Key: item.Text})
		case item.Kind == tt.DatumNumber && item.Value-math.Trunc(item.Value) == 0 &&
			item.Value >= 0 && item.Value <= twoTo64:
			// A whole number up to 2^64, the largest index as a float64
			// rounds to, saturating there.
			index := uint64(maxIndex)
			if item.Value < twoTo64 {
				index = uint64(item.Value)
			}
			path.Segments = append(path.Segments, Segment{Index: index, IsIndex: true})
		default:
			return nil, refuse()
		}
	}
	return path, nil
}

// FormatsJSON is the registry as JSON, one object per format, for `alchemy
// formats`: its id, the shapes it reads and writes, the root its render
// needs, its schema, why its documents are read whole where it says
// (whole, null where it does not), its parts' entries, and its loss
// sentences.
func FormatsJSON() string {
	text := func(s string) tt.Datum { return tt.StringDatum(s) }
	entry := func(a *at.Alc) tt.Datum {
		if a == nil {
			return tt.NullDatum()
		}
		return text(a.Entry)
	}
	formats := Formats()
	items := make([]tt.Datum, len(formats))
	for i, f := range formats {
		p := f.Part
		reads := make([]tt.Datum, len(p.Reads))
		for j, s := range p.Reads {
			reads[j] = text(s.String())
		}
		schema := tt.NullDatum()
		if p.Schema != "" {
			schema = text(p.Schema)
		}
		whole := tt.NullDatum()
		if p.Whole != "" {
			whole = text(p.Whole)
		}
		var render string
		switch p.Render.Kind {
		case at.RenderJSON:
			render = "json"
		case at.RenderCSV:
			render = "csv"
		default:
			render = p.Render.Alc.Entry
		}
		loss := make([]tt.Datum, len(p.Loss))
		for j, l := range p.Loss {
			loss[j] = text(l)
		}
		items[i] = tt.ObjectDatum(
			tt.Member{Key: "id", Value: text(p.ID)},
			tt.Member{Key: "reads", Value: tt.ArrayDatum(reads...)},
			tt.Member{Key: "writes", Value: text(p.Writes.String())},
			tt.Member{Key: "root", Value: text(p.Root.String())},
			tt.Member{Key: "schema", Value: schema},
			tt.Member{Key: "whole", Value: whole},
			tt.Member{Key: "lift", Value: entry(p.Lift)},
			tt.Member{Key: "embed", Value: entry(p.Embed)},
			tt.Member{Key: "render", Value: text(render)},
			tt.Member{Key: "transduce", Value: entry(f.Transducer)},
			tt.Member{Key: "loss", Value: tt.ArrayDatum(loss...)},
		)
	}
	return tt.ArrayDatum(items...).String()
}
