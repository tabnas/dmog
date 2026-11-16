# tabnas-dmog

The `dmog` command (data mogrify): runs programs in the
[alchemy](https://github.com/tabnas/alchemy) language over JSON
documents, and translates a document from any of the tabnas formats into
any other, in TypeScript, Go and Rust.

alchemy is the language and the shared types: it compiles a program into
a plan and lowers the plan onto routers and renderers that its host
passes in. This repository is the host. It composes three components:

- [tabnas-alchemy](https://github.com/tabnas/alchemy): the language, and
  the event protocol every component shares;
- [tabnas-transduce](https://github.com/tabnas/transduce): the routers a
  plan's streams run through, and the parser source that turns the input
  document into events;
- [tabnas-render](https://github.com/tabnas/render): the renderers and
  writers that turn events into text.

A library that only needs one of them depends on that one alone; this
command is where all of them meet.

```text
dmog canon FILE       print the program in canonical form
dmog format FILE      print the program in layout form
dmog check FILE       parse, desugar, resolve, check and build the plan; print nothing and exit 0
dmog explain FILE     print the plan report
dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
                      run the program over the JSON document INPUT; with --grammar, over
                      the rule passes of INPUT parsed as FORMAT (a transducer part's input)
dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
               [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT
                      write INPUT, read as FORMAT, in another format
dmog formats          print the formats translate reads and writes, as JSON
```

`FILE`, `PROGRAM` and `INPUT` may be `-` for standard input (one of them
per run). The worked example of the transducer's design is under
[`examples/`](examples/): `dmog run examples/export.alc
examples/records.json` renders a table bound by the document's own
metadata as CSV, and `dmog run --render json examples/table.alc
examples/records.json` writes the same rows as JSON records. A failure is the `Fail` as one JSON object on standard error,
and standard output carries nothing but the answer. The language
reference, [`docs/language.md`](https://github.com/tabnas/alchemy/blob/main/docs/language.md)
in alchemy, describes each command, the `run` options and the exit
statuses.

`translate` reads `INPUT` with the grammar of `--from` and writes it in
`--to`, through the translation parts each format's package exports:
ABNF (`abnf`), C (`c`), CSS (`css`), CSV, EBNF (`ebnf`), expressions
(`expr`), feeds (`feed`: Atom and RSS, written as Atom), GBNF (`gbnf`),
INI, JSON, JSON5, JSONC, jsonic, JSON Lines, Markdown, PGN (`pgn`),
Protocol Buffers (`proto`), Semantic Versioning (`semver`), TOML, XML,
YAML and ZON. A format takes a document of any other, but for what its
parts declare it cannot carry, which it refuses with
`TARGET_VALUE_UNREPRESENTABLE` (status 1) before writing anything: C,
CSS, PGN and Protocol Buffers write only their own tree, read from a
document of the same format or made by a `--with` program, and Semantic
Versioning writes only a tree that is a version. The grammar notations,
ABNF, EBNF and GBNF, read a document as its rules in source order, which
no compiler has seen, a rule at a time through each notation's transducer
part, and each writes the others' too, so a grammar written in one is
written in another (an ABNF grammar as GBNF), as far as that notation can
say it: what it has no form for (a negated class in ABNF, the engine's own
tokens in GBNF) it refuses, naming the rule and what it met. A notation
that starts from its first rule writes the rule another names as its
start (GBNF's `root`) first. `--path` takes a JSON
array of keys and indexes (`["people",0]`) and translates that value
instead of the document, as a plain tree whatever the source's shapes;
`--key` names the member a root is wrapped under for a format whose
document must be an object (`items` by default); `--with` runs a program
over the input first and writes its export's events or table.

`translate` writes as it reads: through its format's transducer part a
document's output streams from the first record, and nothing waits for
the document's end (admin ADR-33). The command's writer holds no more
than 32 KiB of a record still open, so a long one, a stylesheet's
`rules` written as JSON, say, is written as it comes; a run that fails
part way leaves its output ending with a whole record where the record
was shorter than that. Not every format does so yet: the
streaming gate (`AGENTS.md`) holds each one, in every runtime, to writing
before its input ends and to flat memory over ten times its document, and
names each format that does not, and why. A `.proto` file streams a declaration
at a time, its tree the file's declarations in source order (the schema
`proto-ast`), after the check of its nesting that proto's own parse makes
first; read whole, it is the same tree, which proto's package builds,
since its grammar's own value is the syntax tree. A C translation unit
streams an item at a time, and a function definition a statement at a
time, through C's part (from c 0.6.1), its tree the one c's grammar's
rules build (the schema `c`). A document the part
cannot stream fails with its code part way, after what it wrote, with
`"output": "partial"` in the failure where bytes had left: a repeated key, which
the grammar's value keeps once (`DUPLICATE_MEMBER`, status 1), and a
YAML stream of several documents, two plain `<<` keys in one YAML
mapping (or a plain and a quoted one), a YAML merge source that repeats
a key the grammar merged, a TOML table or an INI section entered again
after another,
and a feed's element after its first entry that changes the feed's own
members, written before the entries (`STREAMABILITY_UNKNOWN`, status
2). The failure's message ends by naming `--whole`, which reads such a
document whole, as its grammar's value, and writes that: a YAML stream
of several documents as the array of them, a repeated key as the
grammar keeps it. A path's value streams as the root does: the document
is read through the same part, and only the events of the value the path
names are written, as they come, so a number keeps the lexeme the
document spelled it with. The rest of the document is still read, and
held to the same refusals, since which of two repeated members a path
names is the grammar's value's to say; a path that names nothing fails
(`INPUT_INVALID`, status 1) once the document has been read, with
nothing written. Under `--whole`, the path selects in the grammar's
value.

`dmog formats` lists each format, as JSON:
its `id`, the shapes it `reads` and `writes`, the `root` its render
needs, its `schema`, `whole` (why its documents are read whole, never
streamed, where the format says so, and null where it does not), its
parts' entries (`lift`, `embed`, `render`, and `transduce`, the
transducer part its documents are read through, null where it ships
none), and its `loss` sentences: what a document written in it does not
keep.

Each command reads `INPUT` through a reader where the route allows it,
in Rust, TypeScript and Go alike. `run` reads `INPUT`, a file or standard
input, a window at a time where the JSON grammar declares `lex.window`
(tabnas/parser's admin ADR-30). The parser then holds a window of a long
document rather than all of it, and the output streams as the parse
goes. `run --grammar` reads `INPUT` the same way: a window at a time
where the grammar declares it, and whole first otherwise. Which grammars
declare it is each package's to say, in the constructor this command
calls in each runtime (`make()` in Rust and TypeScript, `Make()` or
`MakeJsonic()` in Go): JSON, JSON Lines, JSONC, JSON5, CSV, CSS, INI,
TOML, XML, ZON, feeds and PGN do from the releases of 2026-11-09,
Markdown and YAML from their releases of 2026-11-10, jsonic from its
release of 2026-11-12 (`makeTabnas()` in TypeScript), and C from its
release of 2026-11-13 (`MakeC()` in Go). JSON5's check that a document holds a value reads only its leading
white space and comments, so a JSON5 document is read a window at a time
after them. `translate` reads a file or standard input
that way on the route through its format's transducer part, with or
without `--path`, and whole on every other route, under `--whole` among
them. On every route through a
part, the engine's tree is emptied behind the stream for a grammar that
transduce verifies alike pruned and not, so the tree holds no record the
stream has passed. Bytes that are not UTF-8, met part way, fail as a file
that cannot be read (status 2), after any output the document before
them made.

## Install

Each runtime installs a command named `dmog`:

| Runtime | Install |
|---|---|
| TypeScript (Node 24 or later) | `npm install -g @tabnas/dmog` |
| Go | `go install github.com/tabnas/dmog/go/cmd/dmog@latest` |
| Rust (1.85 or later) | `cargo install tabnas-dmog` |

The Go command needs no build tag. `run` reads its document through
JSON's transducer part, and `translate` reads a document through its
format's part, or whole where the format ships none. Neither route needs
transduce's rule-event adapter, which builds only with the
`tabnas_nodecell` tag, so a build with the tag reads every document as a
build without it does.

## Layout

| Path | What it is |
|---|---|
| `ts/` | `@tabnas/dmog`, the `dmog` bin |
| `go/` | `github.com/tabnas/dmog/go`, the `dmog` command |
| `rs/` | `tabnas-dmog`, the `dmog` binary |
| `examples/` | the worked example: `export.alc` renders its table as CSV, `table.alc` leaves the rendering to the command, `records.json` is the document they read |

## Build and test

See [`AGENTS.md`](AGENTS.md). The Rust crate builds against sibling
checkouts of alchemy, transduce, render and the grammars they use;
TypeScript and Go install the published packages, and sibling checkouts
are optional there.

## License

MIT. See [`LICENSE`](LICENSE).
