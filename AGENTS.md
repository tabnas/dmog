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
| `test/fixtures/` | the test files vendored from other tabnas repositories, which all three runtimes read (see *Vendored test fixtures*) |
| `tools/vendor-fixtures.sh` | writes `test/fixtures/` at the releases `go/go.mod` requires |
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
vendored fixtures (`test/fixtures`, below), and fail, naming the path,
when either is missing.
alchemy's `.github/workflows/downstream.yml` runs this repository's two
gates against each change there, with `DMOG_SIBLINGS=1` (see *Build and
test*), so a change to alchemy meets them before it merges. A behaviour of alchemy's that a row can express is pinned in
alchemy's `test/spec/`, not here.

## YAML's round trip

`rs/tests/yaml_roundtrip_test.rs` holds YAML's render to YAML's own
reader: every YAML input of tabnas/yaml's vendored fixtures
(the `test/spec/*.tsv` rows that parse, and every case of its vendored
YAML Test Suite that is not an error case) is read, written through
`translate` as YAML, and read back, and must come back as the same value.
The inputs the reader misreads are a ledger, `READER_DEFECT`, each with
its issue: an input in it must still come back different, so a fix to the
reader fails the test until its line is deleted. It lives here because
tabnas/yaml does not depend on alchemy, and this crate composes the
three components; the fixtures are those of the tabnas/yaml release
`go/go.mod` requires, vendored like the rest.

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
- C, as the realized tree in Rust (`tabnas_c::parse`), and as the value
  of the parser `make()` builds in TypeScript and Go (`MakeC()` in Go). A
  translation unit is read so only where it is read whole: otherwise C's
  transducer part reads it over the rule passes of c's streaming parser
  (`makeStreaming()`, `MakeStreaming()`, `make_streaming()`, from c
  0.6.2), whose passes are those of the parser `make()` builds and which
  builds no tree, so a stream holds no declaration it has closed. It reads
  a window at a time from a reader. So every runtime carries both the
  parser and the tree (`Reader::ParsedTree` in Rust, `parser` with `tree`
  in Go, `{ parser, tree }` in TypeScript);
- Go's feeds and PGN databases, whose modules build typed values, through
  their JSON encoding (`plainTree`). A feed or a PGN database is read so
  only where its transducer part refuses it: otherwise the part reads it
  over the parser's rule passes, as in every runtime, so Go's modules
  carry both the parser and the tree;
- proto's tree, its declarations in source order (`parse_tree`,
  `parseTree`, `ParseTree`), whose parse first refuses a document nesting
  past proto's cap, naming its depth. A `.proto` file is read so only
  where it is read whole: otherwise proto's transducer part reads it over
  the rule passes of its streaming parser (`makeStreaming()`,
  `MakeStreaming()`, `make_streaming()`, from proto 0.7.1), whose passes
  are those of the parser `make()` builds and which builds no tree, so a
  stream holds no declaration it has closed. It reads a window at a time
  from a reader, and refuses a document nesting past the cap as it reads
  the brace past it (`proto_too_deep`, from proto 0.7.0), before the
  engine builds a tree whose drop could abort the process. So every
  runtime carries both the parser and the tree (`Reader::ParsedTree` in
  Rust, `parser` with `tree` in Go, `{ parser, tree }` in TypeScript).

Where a package's own parse checks a document before its parser runs, and
the parser does not check it itself, the reader makes the same check
first (`Reader::Checked` in Rust, `check` in Go): json5 refuses a
document holding no value with its own codes, in Rust and Go. Through a
reader json5's check reads only as far as it needs, a document's leading
white space and comments, and hands the parser a reader that gives what
it read and then the rest, so the document is still read a window at a
time (`check_reader`, `CheckReader`). TypeScript's json5 parser makes
json5's check itself, in its own `parse` and `parseReader` (json5
0.5.18), so TypeScript reads JSON5 with its `make()` alone. proto's
parser counts its nesting itself as it reads, in every runtime, so proto
needs no check of the reader's.

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

## C's part and C's reader

C's transducer part rebuilds the tree c's grammar's rules build, and it
places each operator itself, from its own copy of C's operator table
(admin ADR-33, as amended on 2026-11-13). c holds its copy to the table;
the part's tree is held to the reader's here, since only this repository
runs the part. Each runtime reads every document of c's fixtures
(its `test/spec/*.tsv`, vendored, each once) through the part
and whole, in the plain mode a host reads C in: Rust's release run
(`rs/tests/translate_test.rs`, `Format::read` against `tabnas_c::parse`),
`ts/test/translate.test.ts` and `go/translate/c_part_test.go` (each
`translate` into JSON against `--whole`). Rust's release run reads every
Csmith program of c's corpus (`ts/test/csmith-corpus`) too, each after a
header that declares csmith.h's type names. A program goes through the
part at about 4 KB a second in release, so the programs run on up to four
threads, the longest first. TypeScript and Go leave the Csmith programs
out, where the part runs slower still. c's fixtures and Csmith corpus are
vendored, as every other repository's are (`test/fixtures`, below).

Each document must come out as the same tree, compared as a value, or be
refused alike: the same code, message and position. A tree that nests
past the default `max_depth` is refused on that limit by both routes,
through the part and by `--whole`; Rust then compares its trees with no
limits. The default is 256 levels, and a C tree takes two JSON levels a
node, so this happens past about 128 nodes deep. Each run pins its
counts. Go's whole read writes every node's members in the order its
reader's maps keep them, so in Go every document that parses reads as the
same tree in another member order. A document the part reads otherwise in
TypeScript, for a defect of c's, is registered there with its issue
(`C_PART_DEFECTS`) and held to still reading otherwise, so a fix fails the
test until its entry is deleted; none is registered.

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
  document does, with the same output. The clause sees output only once
  it passes the writer's budget, 32 KiB of a record still open, and the
  engine keeps the window's margin, 64 KiB, unread, so an item's text
  must reach the output: the first nine tenths of a document, less that
  margin, must write more than the budget. A text in a comment the render
  drops sits at that threshold. The grammar notations' items held theirs
  in a comment, and once abnf, ebnf and gbnf read a window at a time,
  abnf wrote 110 bytes past the budget at the cut, and ebnf and gbnf fell
  short of it by under 300; each holds its text in a literal now.
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
  command's live heap after a full collection, sampled as it reads and
  writes, in the test binary run again as the command
  (`go/cmd/dmog/streaming_memory_test.go`), since its peak resident set is
  mostly the runtime and the binary's own pages and swings with the
  collector's headroom. A run is stopped once it is past the bound, which
  is all the clause asks of it. Each bound is calibrated from the
  measurements its comment records, and each runtime holds its bound to
  fail JSON read whole (`--whole`).

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
run in parallel. On four busy cores the gate took three to five minutes in
Rust's debug run and two to three in TypeScript, where the parts run
slowest and csv's run over ten times its document takes longest, and about
a minute in Go, whose memory clause stops every run past the bound.

## Build and test

This repository is standalone: a clone of it alone builds and tests in all
three runtimes. Every tabnas dependency comes from its registry, at its
latest release, and the three name the same releases: `rs/Cargo.toml`
requires each crate from crates.io (`rs/Cargo.lock` pins it), `go/go.mod`
requires each module from the module proxy, and `npm install` takes the
`@tabnas/*` packages from npm. `go/cmd/dmog/fixtures_test.go` holds
`rs/Cargo.toml` to `go/go.mod`, so moving a tabnas requirement means
moving it in both, with `tools/vendor-fixtures.sh` for its test files.
The tests read only the vendored test files (below).

- TypeScript, from `ts/`: `npm install`, then `npm test`.
- Go, from `go/`: `gofmt -l .`, `go vet ./...`, `go test ./...` (with
  `GOWORK=off` when a `go.work` above this checkout would take it in).
- Rust: `ci/rust/run.sh`, which runs `cargo fmt --check`, then the build,
  the tests, the release run of the translation matrix and clippy, all
  `--locked`.
- TypeScript and Go together: `ci/polyglot/run.sh`, which runs this
  package's TypeScript suite, then the Go suite plain and with
  `-tags tabnas_nodecell`, and vet.

**Against sibling checkouts.** To test unreleased changes in alchemy,
transduce, render or a grammar, clone them next to this repository and run
either gate with `DMOG_SIBLINGS=1`. The Rust gate then patches every
tabnas crate `SIBLINGS` (in `ci/rust/run.sh`) names to its checkout, with
a `[patch.crates-io]` it passes to cargo, so the committed manifest never
names a path, and restores `rs/Cargo.lock` afterwards. The polyglot gate
builds the TypeScript siblings in order and links them over the registry
copies, and runs Go in a workspace over the siblings' modules. Both set
`DMOG_FIXTURES` to the checkouts' directory, so the tests read the
siblings' own test files in place of the vendored ones. alchemy's
`.github/workflows/downstream.yml` runs both gates so, for each change to
alchemy.

## Vendored test fixtures

No test reads a file outside this repository. Every test file the three
ports read from another tabnas repository (alchemy's and every format's
`test/spec`, transduce's `rs/tests/fixtures` and `rs/tests/support`, the
grammar notations' example grammars, c's Csmith corpus, and the parts of
JSONTestSuite and the YAML Test Suite that jsonc and yaml vendor) is
vendored into `test/fixtures/<repository>/`, at the path it has there, so
TypeScript, Go and Rust read the same files. `tools/vendor-fixtures.sh`
writes them, each repository at the release this repository builds
against: the tag `go/vX.Y.Z` of the version `go/go.mod` requires, which
npm installs too. `test/fixtures/SOURCES.tsv` records each tag and commit,
and `go/cmd/dmog/fixtures_test.go` fails when a tag is not the version
`go/go.mod` requires, so a requirement that moves without its fixtures
fails by name, not as rows that hold one release's code to another's
fixtures. Moving a tabnas requirement in `go/go.mod` therefore takes
`tools/vendor-fixtures.sh` in the same change. Never edit a vendored file
by hand: a fixture that is wrong is fixed in its repository, released, and
vendored again. `.gitattributes` keeps them byte for byte on every
platform. `DMOG_FIXTURES`, when set, names another directory laid out the
same way, which the tests read instead (the gates set it to the sibling
checkouts' directory with `DMOG_SIBLINGS=1`).

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

## Client work

Never mention or identify a private work-tracking repository, even indirectly,
in this repository or in anything the client can see. This includes code,
comments, documentation, configuration, branch names, commits, issues, pull
requests, chat messages, and visible tool calls or output.

Only plain delivery and task identifiers may be shared from internal tracking.
For example, “Delivery `rounds`, task `restart`.” Keep repository names,
URLs, paths, skill names, project and session identifiers, and transcript
references private. Describe the product work and test results normally.

## Set up the client checkout

`CLAUDE.md` at the repository root is a relative symlink to `AGENTS.md`,
committed, so an agent that reads either name reads this file. On repeat
setup, verify `readlink CLAUDE.md` prints `AGENTS.md`. Do not add a
`CLAUDE.MD` beside it: a name that differs from it only in case collides
on a case-insensitive filesystem, as macOS and Windows checkouts are, and
this repository's CI checks out on both. The target belongs to this
client checkout.

## Track the current session

When you start a delivery, rename the active chat to its plain delivery ID if
the chat host supports renaming. Update the title when you switch deliveries.
Keep the private session key stable. If renaming is unavailable, continue
recording the worked tasks in the same session.

In the deliverable description README, link to the session that worked on it.
Use the host's existing client-accessible conversation URL for a client README.
Keep private tracking links inside the private delivery report. Record the
real chat URL privately when available and link every worked task so the
report can link the session record even when the host supplies no URL.
Keep links to earlier sessions when another session takes over.

Record a timestamped session history entry of `working` when work starts or
resumes. Once the final response has been delivered and work is waiting on the
user, record `awaiting_input`. Keep `working` through progress updates. If
the host exposes delivery only after the turn ends, use its completion hook
or record the observed event on resume. Use `unknown` if activity cannot be
confirmed. Refresh the reports after a transition.

Capture each turn once in the session's private `turns.csv`. Use the capture
helper to maintain `requests.idx`, which lists only the physical CSV line
numbers of user requests. Preserve the exact message, role and turn number.

## Describe the goal

For every private node you create or update, keep both fields short and to the
point. Use as many sentences as needed to make the goal or its state clear.

- **`desc`** states the intended outcome. For a project or delivery, name the
  capability it will provide. For a task, name the result that would finish it.
- **`statusDetail`** states where that goal stands. Say what is possible now
  and, if it affects the plan, name the remaining gap, blocker or decision.

Apply the same rule to the overall collection, sessions and messages. A
session summarizes its goal and where the work stood at handoff. A message
annotation captures the request or decision and its disposition. Preserve
the original message verbatim.

For example, a rounds delivery could have `desc` “Let players play a series
of rounds” and `statusDetail` “Players can start another round. The score still
resets between rounds.”

Keep PR and issue details, links, branch names, commit IDs, implementation
steps and test logs out of both fields. Put supporting references and
verification evidence in private history notes. When updating an older
summary that reads like a work log, condense it to the current goal and outcome.
At handoff, keep the task state and its history in sync. Update a parent
summary when the goal it describes has moved forward or its outlook has changed.

## Before and after a product pull request

1. **Before opening the product PR**, set each task whose implementation is
   ready to submit to `awaiting_merge` in its private record. In its history,
   record what was tested, the readiness time, and the branch or commit. Refresh
   the affected delivery and project summaries and reports. Open a private
   tracking PR, merge it, and confirm the update is on the tracking default
   branch. Only then submit the product PR.
2. **During review**, keep those tasks in `awaiting_merge` while the change
   is ready and waiting. If changes are requested that require more work,
   return the affected tasks to `in_progress` and record the reason. If the
   PR closes unmerged, record whether the work has resumed or returned to
   the queue.
3. **After the product PR merges**, verify the merge on the intended target
   branch. Set each task fully delivered by that merge to `done`. Append the
   actual merge time, product PR reference and merge commit to its private
   history. Refresh the summaries and reports, then open and merge a second
   private tracking PR. Confirm the completed tasks are on the tracking
   default branch.

Keep all tracking operations and references private under the sharing rules
above. The product PR describes the product change and its tests.
