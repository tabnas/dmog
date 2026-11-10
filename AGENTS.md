# Agents Guide — dmog

`CLAUDE.md` is a symlink to this file.

## Core principle: dependencies change only on explicit instruction

**Dependencies may only be changed by explicit instruction from the
maintainer.** This covers every dependency this repository declares, in
every runtime and every manifest:

- `package.json` `dependencies`, `peerDependencies` and `devDependencies`,
  and their lockfiles;
- `go.mod` `require` and `replace` lines, their versions, and `go.sum`;
- `Cargo.toml` dependency tables and `Cargo.lock`;
- any other manifest here, nested test modules included.

Adding, removing, re-pointing or re-versioning any of them is a
dependency change.

- **A dependency never arrives as a side effect.** Watch for an import,
  `go mod tidy`, `npm install`, `cargo update`, a stamped template, or a
  fix for something else. If a change would alter a dependency, stop and
  ask before making it. Do not make it and explain afterwards.
- **An explicit instruction names the change**, for example "bump the
  parser requirement in X to 0.12" or "cascade the parser release". A
  goal is not an instruction for its means. "Make CI green", "ship the C
  library" or "fix the build" does not authorise a dependency change,
  however direct the route through one looks.
- **This repository's own version sites are not dependencies.** They
  include the root entry of its own lockfile. A release bump moves them.
- **Versions track the latest release.** Every dependency is kept at
  its latest published version, and none is held on an older one. That
  is the maintainer's standing instruction, so moving a dependency to
  its latest version needs no further one. Holding a dependency back,
  or adding, removing or re-pointing one, still does.

## Core principle: transient tasks report progress

**Every transient task produces status output at least every 30 seconds,
with an estimate of how far through it is, as a percentage, where one can
be made.** This is the maintainer's instruction. A transient task is any
work that runs for a while and then ends: a build, a test or conformance
sweep, an install or a fetch, a release, a wait on CI, a benchmark, a
script or loop you write, and anything sent to the background.

- **Minimal is enough.** One line with the step and a count, such as
  `conformance: 412 of 1500 (27%)`, meets it. When no total is known, print
  what is known (the step, the current item, the elapsed time) and say the
  percentage is unknown rather than inventing one.
- **Build it into what you write.** A script or loop prints a line per
  item or per interval. A quiet tool gets its progress or verbose flag, or
  a wrapper that prints a heartbeat, so that nothing runs silent for more
  than 30 seconds.
- **Silence reads as a hang.** Whoever is watching, a person or an agent,
  cannot tell a slow task from a stuck one without it, and so cannot
  decide whether to wait or to stop it.

A quick command that finishes within 30 seconds needs nothing extra.

## Pull requests

Open pull requests **ready for review, never as drafts.** This is a
standing maintainer preference, and it overrides any tooling or agent
default that opens pull requests in draft state.

## What this project is

The `dmog` command (data mogrify), in TypeScript, Go and Rust: `canon`, `format`,
`check`, `explain` and `run`, as alchemy's
[`docs/language.md`](https://github.com/tabnas/alchemy/blob/main/docs/language.md)
describes them, and `translate` and `formats`, which write a document of
any tabnas format with translation parts in any other, composed by
alchemy's `translate` from the parts each format's package exports
(admin ADR-27; the README has the options). The name is short for
*data mogrify*; the command was `alchemy`, in `tabnas/alchemy-cli`,
until 2026-10-25 (admin ADR-32), and aless once carried its exports and
programs (until aless 0.5.0). It is the composition root
of three components that do not depend on each other's implementations:

- **alchemy** holds the language and the shared types: the event
  protocol (events, sinks, tables, `Fail` and its codes, limits,
  selectors, datums) and the `Routers` and `Renderers` interfaces a
  compiled program is lowered through.
- **transduce** depends on alchemy's shared types and implements
  `Routers`; its parser source reads the input document.
- **render** depends on alchemy's shared types and implements
  `Renderers`; its writers put the text on standard output.

The command builds a transduce `Routers` and a render `Renderers`,
passes both to alchemy's `compile`, and runs the result over the input.
Nothing else in the fleet constructs all three, so behaviour that needs
all three belongs here, and a library that needs one of them depends on
that one alone. The policy of the command's own output is the host's too:
it wraps render's renderers in one place in each runtime
(`translate::renderers`, `translate.Renderers`, `renderers` in
`ts/src/translate.ts`), every method render's but the writer's, which
holds at most render's budget (32 KiB) of a record still open, where
render's default holds 16 MiB. `translate` streams from its first byte
(admin ADR-33, ruling 1), and render's JSON writer makes each member of a
root object one record, so of a tree whose bulk is one member of its root
(a stylesheet's rules, an XML element's children) render's default held up
to 16 MiB before it wrote a byte, which the streaming gate found.

## Layout

| Path | What it is |
|---|---|
| `ts/` | `@tabnas/dmog`: the `dmog` bin and its tests, and the tests of alchemy's programs (`ts/test/host.ts` composes the three components for them) |
| `go/` | `github.com/tabnas/dmog/go`: the `dmog` command and its tests |
| `go/e2e/` | the tests of alchemy's programs in Go: test files only, alchemy's exported API composed with transduce and render |
| `rs/` | `tabnas-dmog`: the `dmog` binary and its tests, and the tests of alchemy's programs (`rs/tests/common/` composes the three) |
| `examples/` | the worked example of the transducer's design (`export.alc`, `table.alc`, `records.json`), which `rs/tests/cli_test.rs` runs as the README says |
| `test/` | what the three runtimes' tests share: `notation-samples.json` (the translation matrix) and `streaming.json` (the streaming gate) |
| `ci/rust/run.sh` | the Rust gate `.github/workflows/rust.yml` runs |
| `ci/polyglot/run.sh` | the TypeScript and Go gate the same workflow runs |
| `ci/phase.sh` | `phase`, which both gates run each step through: a line every 25 seconds while it runs |

## The tests of alchemy's programs

alchemy depends on neither transduce nor render (the maintainer's ruling
of 2026-10-08), so its releases and theirs never leave a requirement a
release behind. Its tests that run programs need all three, so they are
here, in every runtime:
- `run.tsv`;
- the catalogue test over every error row of alchemy's four fixture files;
- the lowering;
- events;
- linked sources;
- the translation parts;
- the standard library's text over the fixtures and the generated
  documents: what its compositions and the parts write, held to
  transduce's table transducer with render's CSV renderer, and to the
  grammars' own values.

They read alchemy's `test/spec/` and transduce's `rs/tests/` from the
sibling checkouts, and fail, naming the path, when either is missing.
alchemy's `.github/workflows/downstream.yml` runs this repository's two
gates against each change there, so a change to alchemy meets them before
it merges. A behaviour of alchemy's that a row can express is pinned in
alchemy's `test/spec/`, not here.

## YAML's round trip

`rs/tests/yaml_roundtrip_test.rs` holds YAML's render to YAML's own
reader: every YAML input of the sibling tabnas/yaml checkout's fixtures
(the `test/spec/*.tsv` rows that parse, and every case of its vendored
YAML Test Suite that is not an error case) is read, written through
`translate` as YAML, and read back, and must come back as the same value.
The inputs the reader misreads are a ledger, `READER_DEFECT`, each with
its issue: an input in it must still come back different, so a fix to the
reader fails the test until its line is deleted. It lives here because
tabnas/yaml does not depend on alchemy, and this crate composes the
three components; the fixtures are the sibling checkout's, the same one
`rs/Cargo.toml` takes tabnas-yaml from, so they are the reader's own.

## The translation matrix

`translate`'s tests (`rs/tests/translate_test.rs`,
`ts/test/translate.test.ts`, `go/translate/translate_test.go`) run the
cross product of their corpus into every format: transduce's fixtures and
JSONTestSuite's documents in every runtime, the grammar notations'
example grammars in TypeScript's and Go's run and in Rust's release run,
and every format's own `test/spec/*.tsv` in Rust's release run
(`ci/rust/run.sh`), the grammar notations' among them (ABNF's grammar
rows, and each notation's `tree.tsv`). A pair is held to what its target
declares: written, and read back under the target's conventions (its
loss list); or refused with the code and the reason its parts declare,
which the matrix counts: a schema-only target (one that writes a
schema's tree with no embedding into it: C, CSS, PGN, proto and the
grammar notations) refuses another format's tree, a grammar notation's
render refuses a grammar it has no form for, naming the rule and what it
met, and Semantic Versioning's embedding refuses a tree that is not a
version. A pair refused otherwise, or written where a refusal is
declared, fails. `translate` streams its output, so a document the
source's part cannot stream (a repeated key, a stream of several YAML
documents, two plain `<<` in one YAML mapping, a YAML merge source that
repeats a merged key) fails part way, naming `--whole` (admin ADR-33):
the matrix then reads it with `--whole`, holds its pairs to what that
writes, and counts it, each run with a ceiling on how many it reads so.
A document is left out as a source where it is past
what every format takes: nested deeper than 100 levels, or holding more
values than the run's size bound (10,000 by default, and 100,000 in
Rust's release run, past which no document of the corpora is now that
the grammar notations are read as their rules); each run pins how many
it leaves out.

The grammar notations, ABNF, EBNF and GBNF, share the schema
`grammar-rules`, a grammar's rules in source order, which tabnas/bnf
defines and each notation's reader builds without compiling them, so
each writes the others' documents. A grammar written in a notation is
held to the round trip each render declares: from the same notation it
reads back as the tree it was; from another notation, written as the
target spells what the source spells otherwise (its loss list), it reads
back as a tree the render writes again as the same text. A render
refuses what its notation cannot say with TARGET_VALUE_UNREPRESENTABLE,
naming the rule (`the grammar cannot be written as ABNF: ` and its
twins), among it a `start` the grammar does not define (a GBNF document
of rules none of which is `root`, which every render refuses, its own
included), and, in GBNF, a tree with no start whose rule named `root` is
not its first, and a reference to the engine's own NR, ST, TX or VL that
the grammar does not define. And every grammar written is held to what
it recognises: the document's text and the text written, each compiled
by its own notation's compiler and installed on an engine of its own,
parse each of the document's samples alike. A document its own compiler
refuses, which its reader reads all the same, has nothing to compare,
and each run pins how many pairs with samples it skips so (`uncompiled`).
The samples are the inputs the notations' repositories' own tests give
their example grammars, accepted and refused alike
(`test/notation-samples.json`, which all three runtimes read), and, in
Rust's release run, the inputs ABNF's fixture rows give each grammar.
The one difference the loss lists declare is the lexing: a grammar of
another notation compiles under the target's settings, so across GBNF's
exact lexing and the others' default one, which skips white space, a
sample holding white space may be recognised otherwise. Such a pair is
registered, never passed: the same file's `otherwise` names each pair
with exactly the samples it recognises otherwise, every cross product
holds each pair it compares to that list, and Rust's release run, which
compares every pair named, holds every entry to a pair it compares. A
grammar recognising any other sample otherwise fails, unless its pair is
registered with its package's defect (`DEFECTIVE_PAIRS`): one is, ABNF's
`alignment-abnf-ast.tsv:16` into EBNF, whose `c ::= ( "y" )?` tabnas/bnf
compiles to a grammar that accepts nothing (tabnas/bnf#123).

A format is read with its package's parser, through transduce's parser
source (the grammar notations among them, whose `make()` builds the
parser whose value is their tree, read through their transducer parts),
unless the parser's own value is not yet the tree its parts declare and
the package's API reads a document as that tree: then that tree is the
document, read whole (`Reader::Tree` in Rust, `tree` in Go, `{ tree }` in
TypeScript), with the checks the package's API makes first:
- expressions, as the simplified tree (`parse_simplified`,
  `parseSimplified`, `SimplifyOrdered`);
- C, as the realized tree in Rust (`tabnas_c::parse`); TypeScript and Go
  walk the parser's value;
- Go's feeds and PGN databases, whose modules build typed values, through
  their JSON encoding (`plainTree`). A feed or a PGN database is read so
  only where its transducer part refuses it: otherwise the part reads it
  over the parser's rule passes, as in every runtime, so Go's modules
  carry both the parser and the tree;
- proto's tree, its declarations in source order (`parse_tree`,
  `parseTree`, `ParseTree`), whose parse first refuses a document nesting
  past proto's cap, before the engine builds a tree whose drop could
  abort the process. A `.proto` file is read so only where it is read
  whole: otherwise proto's transducer part reads it over the rule passes
  of the parser `make()` builds, after the same check, so every runtime
  carries both the parser and the tree (`Reader::CheckedTree` in Rust,
  `parser` with `tree` in Go, `{ parser, check, tree }` in TypeScript).

Where a package's own parse checks a document before its parser runs, and
the parser does not check it itself, the reader makes the same check
first (`Reader::Checked` and `Reader::CheckedTree` in Rust, `check` in
Go, `{ parser, check, tree }` in TypeScript): json5 refuses a document
holding no value with its own codes, and proto a document nesting past
its cap (`preflight`, `Preflight`), in every runtime. Through a reader
json5's check reads only as far as it needs, a document's leading white
space and comments, and hands the parser a reader that gives what it read
and then the rest, so the document is still read a window at a time
(`check_reader`, `CheckReader`); proto's counts the whole document, which
is read whole first, as proto's parser, declaring no window, reads it
anyway. TypeScript's json5 parser makes json5's check itself, in its own
`parse` and `parseReader` (json5 0.5.18), so TypeScript reads JSON5 with
its `make()` alone.

A format whose reader, as this command reads a document through
transduce, does not build the tree its parts declare is registered in
that runtime's test with its defect (`READER_DEFECTS`, `readerDefects`):
its documents are left out as sources and counted, and a test holds each
entry to an example, comparing what the command reads with what the
format's package builds with its own API (for a version past 2^53 - 1,
its own round trip), so an entry fails once the reader is repaired, and
is then deleted. Go also pins the order a reader's objects are written
in where it is not the Rust and TypeScript readers' (`orderDivergent`:
plain maps in sorted key order, and typed values in their fields'
order), which the matrix, comparing values, cannot see. (The grammar
spec was one until tabnas-bnf 0.1.30, whose Go serializer wrote it in
name order, its match tokens' order in a list of its own; it now writes
the order the compiler emits, as TypeScript and Rust do.) A defect is the
package's to repair; never register one to make a pair pass without
naming it, and never work around one here in silence.

## The streaming gate

`rs/tests/streaming_test.rs`, `ts/test/streaming.test.ts` and
`go/cmd/dmog/streaming_test.go` hold every format the formats table lists
to the definition of streaming in alchemy's `docs/streaming-plan.md`,
section 13 (admin ADR-33): in every runtime, a document of it read from a
file and from standard input has written its first byte before its input
ends, and the run's peak memory stays flat over ten times the document.
The documents are `test/streaming.json`'s, which all three read: for each
format, a document of `items` items, each a short distinct name and a long
plain text, so that the document is longer than what the lexer's window
holds at a few events an item, and one of ten times as many. Each runtime
translates them to JSON, `dmog translate --from F --to json`, and holds
each format to two clauses:

- **written before the end**: the document is handed over in two parts,
  from standard input and from a named pipe given as INPUT (a FIFO, which
  `mkfifo` makes; Windows has none and runs standard input alone), and the
  command must write after the first part, nine tenths of the items,
  before the rest is sent. The run then ends as one over the whole
  document does, with the same output.
- **flat memory**: what the run holds over ten times the document stays
  within the runtime's bound of what it holds over the document: 1.25
  times it in Rust, 1.5 times in TypeScript and Go. Each runtime measures
  what it can without a dependency: Rust in process, a counting global
  allocator over `translate::run_document`, the call the binary makes, per
  thread, with the document made as it is read; TypeScript the command
  with a preload (`ts/test/streaming-memory.ts`) that samples the live
  heap, less the code V8 has compiled, and the buffers outside it as the
  command reads and writes (V8's peak resident set follows what the parts
  allocate, not what they hold), and runs it with V8's background threads
  off (`--single-threaded`): otherwise what a sample finds alive depends
  on when an optimization compiled on another thread is installed, which
  on a busy machine swelled a sample by as much as 9.5 MB; Go the
  command's peak resident set (`ru_maxrss`, unix), which on Linux it also
  reads while the command runs. A run is stopped once it is past the
  bound, which is all the clause asks of it. Each bound is calibrated from
  the measurements its comment records, and each runtime holds its bound
  to fail JSON read whole (`--whole`).

A format that fails either clause is registered in that runtime's ledger
(`NOT_STREAMING`, `notStreaming`) with the clauses it fails and why, an
issue or a ruling, and the gate holds each entry to failing exactly those
clauses: a format that starts streaming, or passes a clause its entry
names, fails the gate, which says so, until the entry or the clause is
deleted; one that stops streaming fails it until it is registered, with
why. A format the table lists with no document in `test/streaming.json`
fails it too, and so does an entry or a document that names no format the
gate runs. expr and semver, which the maintainer ruled whole (ADR-33,
ruling 4), are listed apart (`RULED_WHOLE`, `ruledWhole`) and are not
run, and the gate holds the table to reading them whole. Never widen a
bound, or shorten a document, to pass a format: register it.

Three formats are held at once in Rust and TypeScript, and Go's formats
run in parallel, one measured run at a time. On four busy cores the gate
took three to five minutes in Rust's debug run and two to three in
TypeScript, where the parts run slowest and csv's run over ten times its
document takes longest, and under a minute in Go, whose memory clause
stops every run past the bound.

## Build and test

The Rust crate builds against sibling checkouts of alchemy, transduce,
render and the grammars they use, cloned next to this repository:
`rs/Cargo.toml` takes them by path, and `ci/rust/run.sh` lists every one
the graph needs. They are all on crates.io, but the committed manifest
stays path-only. TypeScript and Go install the published packages
instead: `npm install` takes the `@tabnas/*` devDependencies from the npm
registry, and `go/go.mod` requires released versions from the module
proxy. For those two, sibling checkouts are optional as code:
`ci/polyglot/run.sh` links them over the registry copies, and admin's
`scripts/link.sh` does the same locally. They are not optional as
fixtures: the tests of alchemy's programs read `../alchemy` and
`../transduce`, as above. The CI workflows name the list.

- TypeScript, from `ts/`: `npm install`, then `npm test`.
- Go, from `go/`: `gofmt -l .`, `go vet ./...`, `go test ./...`, with a
  `go.work` outside the repositories when the siblings carry unreleased
  changes.
- Rust: `ci/rust/run.sh`, which runs `cargo fmt --check`, the build, the
  tests and clippy, then compares `rs/Cargo.lock` with what it was before:
  the gate fails if cargo rewrote anything but the versions of the sibling
  crates, which move with the checkouts. That comparison is why the gate
  does not pass `--locked`, as alchemy's, transduce's and render's gates
  do not.
- TypeScript and Go together: `ci/polyglot/run.sh`, which builds the
  TypeScript siblings in order and runs this package's suite, then the Go
  suite plain and with `-tags tabnas_nodecell`, and vet.

## Releasing

`.tabnas-kind` says `TOOL`, so the release workflows publish (admin
ADR-20). A release is two steps, and a session's credentials cannot push
tags, so neither step pushes one:

1. **A reviewed bump PR** that moves every version site together:
   `ts/package.json`, `VERSION` in `ts/src/cli.ts` and in
   `go/cmd/dmog/main.go`, and `version` in `rs/Cargo.toml` with the
   root `tabnas-dmog` entry of `rs/Cargo.lock` (the binary's
   `VERSION` reads it). `ts/test/version.test.ts`,
   `go/cmd/dmog/version_test.go` and `rs/tests/version_test.rs` fail
   when they disagree.
2. **Dispatch `release.yml` on `main`** with `go` true, once `main` CI is
   green on the bump commit. It publishes npm over OIDC, writes the
   `ts/v` and `go/v` tags, makes the GitHub Release on the `go/v` tag, and
   publishes the crate through `crates-release.yml`. Both trusted
   publishers name `release.yml`.

alchemy, transduce and render are released before this package (admin
`publish.sh` ORDER), since the build resolves them from the registry.
