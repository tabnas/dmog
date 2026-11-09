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
ABNF, EBNF and GBNF, read a document as the grammar spec it compiles to,
and each writes the others' too, so a grammar written in one is written
in another (an ABNF grammar as GBNF), as far as that notation can say
it: what it has no form for (a negated class in ABNF, the engine's own
tokens in GBNF) it refuses, naming what it met. `--path` takes a JSON
array of keys and indexes (`["people",0]`) and translates that value
instead of the document; `--key` names the member a root is wrapped
under for a format whose document must be an object (`items` by
default); `--with` runs a program over the input first and writes its
export's events or table.

`translate` writes as it reads: through its format's transducer part a
document's output streams from the first record, and nothing waits for
the document's end (admin ADR-33). A document the part cannot stream
fails with its code part way, after what it wrote, with `"output":
"partial"` in the failure where bytes had left: a repeated key, which
the grammar's value keeps once (`DUPLICATE_MEMBER`, status 1), and a
YAML stream of several documents, a YAML merge key or alias to a
collection, a TOML table or an INI section entered again after another,
and a feed's element after its first entry that changes the feed's own
members, written before the entries (`STREAMABILITY_UNKNOWN`, status
2). The failure's message ends by naming `--whole`, which reads such a
document whole, as its grammar's value, and writes that: a YAML stream
of several documents as the array of them, a repeated key as the
grammar keeps it.

`dmog formats` lists each format, as JSON:
its `id`, the shapes it `reads` and `writes`, the `root` its render
needs, its `schema`, `whole` (why its documents are read whole, never
streamed, where the format says so, and null where it does not), its
parts' entries (`lift`, `embed`, `render`, and `transduce`, the
transducer part its documents are read through, null where it ships
none), and its `loss` sentences: what a document written in it does not
keep.

The Rust command reads `INPUT` through a reader where the route allows
it. `run` reads `INPUT`, a file or standard input, a window at a time,
since the JSON grammar declares `lex.window` (tabnas/parser's admin
ADR-30). The parser then holds a window of a long document rather than
all of it, and the output streams as the parse goes. `run --grammar`
reads `INPUT` the same way: a window at a time where the grammar declares
it (JSON, JSONC, JSON Lines and CSV), and whole first otherwise.
`translate` reads a file or standard input that way on the route through
its format's transducer part, and whole on every other route, under
`--whole` among them. On every route
through a part, the engine's tree is emptied behind the stream for a
grammar that transduce verifies alike pruned and not, so the tree holds
no record the stream has passed. Bytes that are not
UTF-8, met part way, fail as a file that cannot be read (status 2), after
any output the document before them made. The TypeScript and Go commands
read `INPUT` whole.

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
