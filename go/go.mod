module github.com/tabnas/dmog/go

go 1.24.7

// The language, the stages its programs run on (transduce's routers and
// render's renderers), and the grammars: the JSON one `run` reads its
// document with, and the twenty-two `translate` reads and writes, each
// with its translation parts. The shared types, and the Routers and
// Renderers that transduce and render build on them, are in all three from
// v0.2.0.
require (
	github.com/tabnas/abnf/go v0.4.25
	github.com/tabnas/alchemy/go v0.3.1
	github.com/tabnas/c/go v0.5.15
	github.com/tabnas/chess/go v0.1.18
	github.com/tabnas/css/go v0.5.18
	github.com/tabnas/csv/go v0.6.9
	github.com/tabnas/ebnf/go v0.1.19
	github.com/tabnas/expr/go v0.5.19
	github.com/tabnas/feed/go v0.6.19
	github.com/tabnas/gbnf/go v0.1.20
	github.com/tabnas/ini/go v0.5.21
	github.com/tabnas/json/go v0.5.19
	github.com/tabnas/json5/go v0.5.17
	github.com/tabnas/jsonc/go v0.5.16
	github.com/tabnas/jsonic/go v0.7.10
	github.com/tabnas/jsonl/go v0.1.19
	github.com/tabnas/markdown/go v0.7.13
	github.com/tabnas/proto/go v0.6.11
	github.com/tabnas/render/go v0.2.6
	github.com/tabnas/semver/go v0.0.11
	github.com/tabnas/toml/go v0.5.20
	github.com/tabnas/transduce/go v0.4.1
	github.com/tabnas/xml/go v0.7.19
	github.com/tabnas/yaml/go v0.5.28
	github.com/tabnas/zon/go v0.5.18
)

// What alchemy's end-to-end tests (e2e) need besides, as they read
// documents with the grammars above and run fixtures: the engine and the
// shared fixture runner. They were alchemy's own tests, which ran programs
// on transduce and render; alchemy depends on neither, so they are here.
// translate imports the engine too, to read ZON's big integers in the form
// ZON's translation part declares (zonParser).
require (
	github.com/tabnas/parser/go v0.12.16
	github.com/tabnas/support/go v0.3.9
)

require (
	github.com/tabnas/bnf/go v0.1.31 // indirect
	github.com/tabnas/hoover/go v0.3.14 // indirect
)
