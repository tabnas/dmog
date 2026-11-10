/* Copyright (c) 2026 tabnas, MIT License */

// `dmog translate`: a document in one of the formats whose grammar
// packages this command carries, written in any of them (the Rust crate's
// rs/src/translate.rs).
//
// Every format's package exports its translation parts: its manifest's
// `translate` object (the shapes it reads and writes, the root its render
// needs, the schema its events carry), its render, and, where it has them,
// its lift and its embedding, each an alchemy file. This module reads them
// into alchemy's `Part`s, has alchemy's `translate` namespace compose the
// route (the source's lift, the root adapters, the embedding, the inferred
// table, the render), compiles the composition with transduce's routers
// and render's renderers, and runs it over the input read with the source
// format's grammar. A package that ships a transducer part (its manifest's
// `translate.transduce`, the alchemy file whose entry makes the format's
// JSON events of the parser's rule passes) is read through it: the route
// is composed over the transducer (`composeParsed`), and the input's rule
// passes stream through transduce's `ParserEventSource` into the program,
// whatever the grammar, with nothing held but what the part keeps, and the
// output is written as the run goes. Any other format is parsed whole and
// its value walked, and so is a document the request asks read whole
// (`--whole`). A path below the root selects a sub-stream: the document is
// read as for the root, through its part where the package ships one, and
// only the events of the value the path names are translated, as they
// come (`runPath`). A document
// whose events are not what the grammar's value holds, which the part
// refuses part way (a YAML stream of several documents, two plain `<<` in
// one mapping, a merge source that repeats a merged key, a repeated
// member, which a part streams twice and `tree-contract` refuses), fails with its code after what it wrote, since the output has
// left: `--whole` reads it whole, its grammar's value the document (admin
// ADR-33). Where a package's parser
// builds a value that is not yet the tree its parts declare, and the
// package's API reads a document as that tree (expr's simplified form,
// and the grammar spec a grammar notation's document compiles to), that
// tree is the document, read whole; where such a package ships a
// transducer part too (proto, whose parser's value is its CST), the part
// reads the parser's rule passes, and a document read whole is that tree.
// Nothing here knows a format by its name: a format is what its manifest
// says, and a package whose manifest names no parts this host can take is
// not a format here.
//
// A document may be a file (`{ file }`) or a stream (`{ reader }`,
// standard input): the transducer part's route reads it through a reader
// (transduce's `ParserEventSource.fromReader`), a window of it at a time
// where the source's grammar declares `lex.window` (tabnas/parser's admin
// ADR-30), the engine's arrays emptied behind the stream where transduce
// verifies the grammar alike pruned and not, and every other route reads
// it whole.

import { closeSync, openSync, readFileSync, readSync } from 'node:fs'

import { abnfCompile, toJsonic, toPureSpec, translate as abnfParts } from '@tabnas/abnf'
import { Program, compile as compileProgram, compileSources, isFail, translate } from '@tabnas/alchemy'
import type { CompileOptions } from '@tabnas/alchemy'
import { C, translate as cParts } from '@tabnas/c'
import { make as makeChess, translate as chessParts } from '@tabnas/chess'
import { make as makeCss, translate as cssParts } from '@tabnas/css'
import { make as makeCsv, translate as csvParts } from '@tabnas/csv'
import { ebnfConvert, translate as ebnfParts } from '@tabnas/ebnf'
import { Expr, parseSimplified, translate as exprParts } from '@tabnas/expr'
import { make as makeFeed, translate as feedParts } from '@tabnas/feed'
import { gbnfConvert, translate as gbnfParts } from '@tabnas/gbnf'
import { make as makeIni, translate as iniParts } from '@tabnas/ini'
import { make as makeJson, translate as jsonParts } from '@tabnas/json'
import { make as makeJson5, translate as json5Parts } from '@tabnas/json5'
import { make as makeJsonc, translate as jsoncParts } from '@tabnas/jsonc'
import { jsonic, translate as jsonicParts } from '@tabnas/jsonic'
import { make as makeJsonl, translate as jsonlParts } from '@tabnas/jsonl'
import { make as makeMarkdown, translate as markdownParts } from '@tabnas/markdown'
import { Tabnas } from '@tabnas/parser'
import {
  make as makeProto,
  parseTree as protoParseTree,
  preflight as protoPreflight,
  translate as protoParts,
} from '@tabnas/proto'
import { DEFAULT_BUDGET, WriteOut, renderers as renderRenderers } from '@tabnas/render'
import type { Writer } from '@tabnas/render'
import type { Renderers } from '@tabnas/alchemy/shared'
import { Semver, translate as semverParts } from '@tabnas/semver'
import { make as makeToml, translate as tomlParts } from '@tabnas/toml'
import {
  AbortFlag,
  Datum,
  DatumBuilder,
  Ev,
  Fail,
  Guarded,
  Limits,
  Metrics,
  ParserEventSource,
  ParserSource,
  TreeContract,
  engineFailure,
  isPrunable,
  routers,
  toText,
  walkValue,
} from '@tabnas/transduce'
import type { Flow, JsonEvent, Segment, Sink } from '@tabnas/transduce'
import { make as makeXml, translate as xmlParts } from '@tabnas/xml'
import { make as makeYaml, translate as yamlParts } from '@tabnas/yaml'
import { make as makeZon, translate as zonParts } from '@tabnas/zon'

// The renderers every program this command compiles is lowered onto:
// render's own, every method render's, but for the writer of the text a
// command writes (`writeOut`), which holds at most render's budget
// (`DEFAULT_BUDGET`, 32 KiB) of a record that is still open, where render's
// own holds up to its default hold, 16 MiB of it. `translate` streams its
// output from its first byte (admin ADR-33, ruling 1), and render's JSON
// writer makes each member of a root object one record, so with render's
// hold a tree whose bulk is one member of its root (a stylesheet's rules,
// an XML element's children, a feed's entries, a Markdown document's
// blocks, a TOML array of tables) was written only once 16 MiB of it was
// held, or at the document's end: the streaming gate found it
// (test/streaming.test.ts). A record of up to the budget is still written
// whole when a run fails part way; a longer one is written as it comes, and
// the writer retains at most the budget and the hold. render's default
// suits a library; the policy is the host's.
export const renderers: Renderers = Object.freeze({
  ...renderRenderers,
  writeOut: (writer: Writer) => new WriteOut(writer).withHold(DEFAULT_BUDGET),
})

// What every composition is compiled with: transduce's routers and the
// renderers above.
const OPTIONS: CompileOptions = { routers, renderers }

// A grammar package's parts, as each one's `translate()` hands them over.
// Every package owns its interface types, so this reads the same fields
// from each.
type Parts = {
  readonly manifest: string
  readonly lift?: translate.PartText
  readonly embed?: translate.PartText
  readonly render?: translate.PartText
  readonly transduce?: translate.PartText
}

// A package's parts as alchemy's descriptor, with the transducer part its
// manifest may name beside it: the descriptor carries none, and
// `Part.transducer` reads the two together.
type Described = {
  readonly descriptor: translate.Descriptor
  readonly transduce?: translate.PartText
}

// How a format's documents are read: with its grammar's parser, a fresh
// one per source (a source owns its parser), which makes any check its
// package's parse makes first in its own `parse` and `parseReader` (json5's
// requireValue rule, from json5 0.5.18); or as the tree its package's own
// API reads a document as, where its parser's value is not yet the tree its
// parts declare; or both, where such a package ships a transducer part
// (proto): the part reads the rule passes of its parser, after a check its
// package's own parse makes first, which the parser does not make itself,
// and a document read whole is the tree.
type Reading =
  | (() => Tabnas)
  | {
      readonly parser: () => Tabnas
      readonly check: (text: string) => void
      readonly tree: (text: string) => unknown
    }
  | { readonly tree: (text: string) => unknown }

// A synchronous pull reader of bytes, as the engine's `parseReader` and
// transduce's `ParserEventSource.fromReader` take it: `read(buf)` writes up
// to `buf.length` bytes into `buf` from its start and answers how many it
// wrote, 0 at the end of the input; a read that fails throws.
export type Reader = { read(buf: Uint8Array): number }

// What a translation reads: a text, read whole; a file, which the
// transducer part's route reads through a reader, a window of it at a time
// where the source's grammar declares `lex.window`, so that the run holds a
// window of the document rather than all of it, and every other route
// reads whole; or a stream, such as standard input, read once, through a
// reader on the part's route, as a file is, and whole on every other.
export type Document = { readonly text: string } | { readonly file: string } | { readonly reader: Reader }

// The size of one read of a reader read whole.
const CHUNK = 64 * 1024

// A file's bytes, one read at a time, as a reader: what the engine reads a
// window of. The file is opened by `openFile`, and closed by its caller.
function fileReader(fd: number): Reader {
  return { read: (buf: Uint8Array) => readSync(fd, buf, 0, buf.length, null) }
}

// The text of a reader read to its end. A read that fails, or bytes that
// are not UTF-8, fail as the engine's `unreadable` fails a parse from a
// reader (`cannotRead`).
function readAll(reader: Reader): string {
  const chunks: Uint8Array[] = []
  const buf = new Uint8Array(CHUNK)
  for (;;) {
    let n: number
    try {
      n = reader.read(buf)
    } catch (err: any) {
      throw cannotRead(err?.message ?? String(err))
    }
    if (0 === n) break
    chunks.push(buf.slice(0, n))
  }
  return decode(Buffer.concat(chunks))
}

// A leading byte order mark is kept, as Rust's `read_to_string` keeps it:
// whether a document may begin with one is its grammar's to say.
const UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

function decode(bytes: Uint8Array): string {
  try {
    return UTF8.decode(bytes)
  } catch (_err) {
    throw cannotRead('stream did not contain valid UTF-8')
  }
}

// A file's text, read whole; one that cannot be read, or that holds bytes
// that are not UTF-8, fails as a reader that fails does (`cannotRead`).
function readFile(file: string): string {
  let bytes: Buffer
  try {
    bytes = readFileSync(file)
  } catch (err: any) {
    throw cannotRead(err?.message ?? String(err))
  }
  return decode(bytes)
}

// A document's text, read whole.
function textOf(document: Document): string {
  if ('text' in document) return document.text
  if ('file' in document) return readFile(document.file)
  return readAll(document.reader)
}

// A document that could not be read, failed as the engine's `unreadable`
// fails a parse from a reader once transduce reports it: `INPUT_INVALID`,
// the code and why (`unreadable` reads it back).
function cannotRead(reason: string): Fail {
  return Fail.input(`unreadable: ${reason}`)
}

// Why a run's document could not be read, where that is why it failed: a
// read that failed, or bytes that are not UTF-8, which a parse from a
// reader meets where they are (the engine's `unreadable`, which transduce
// reports as `INPUT_INVALID`, the engine's code before its message), and
// a file read whole that could not be. A host reports such a document as a
// file it could not read, not as one that does not parse.
export function unreadable(err: unknown): string | undefined {
  if (!isFail(err) || 'INPUT_INVALID' !== err.code) return undefined
  const prefix = 'unreadable: '
  if (!err.message.startsWith(prefix)) return undefined
  const reason = err.message.slice(prefix.length)
  const engine = 'the source could not be read: '
  return reason.startsWith(engine) ? reason.slice(engine.length) : reason
}

// A document's events, as a format reads it: the builder transduce's
// ParserSource has, which TreeSource has too.
interface DocumentSource {
  limits(limits: Limits): DocumentSource
  metrics(metrics: Metrics): DocumentSource
  run(sink: Sink): Flow
}

// The tree a package's API reads a document as, read whole and walked as
// the ParserSource walks a parser's value, under the same limits and
// counts; a failure to read it is reported as the ParserSource reports an
// engine error.
class TreeSource implements DocumentSource {
  private sourceLimits: Limits = Limits.default()
  private sourceMetrics: Metrics = new Metrics()

  constructor(private readonly read: () => unknown) {}

  limits(limits: Limits): this {
    this.sourceLimits = limits
    return this
  }

  metrics(metrics: Metrics): this {
    this.sourceMetrics = metrics
    return this
  }

  run(sink: Sink): Flow {
    const abort = new AbortFlag()
    const guarded = new Guarded(sink, this.sourceLimits, abort, this.sourceMetrics)
    try {
      let tree: unknown
      try {
        tree = this.read()
      } catch (err) {
        throw engineFailure(err, abort)
      }
      const flow = walkValue(tree, guarded)
      return 'continue' === flow ? guarded.event(Ev.end) : flow
    } finally {
      guarded.flush()
    }
  }
}

// One format this command reads and writes: its parts, as its package's
// manifest names them, how its documents are read, and the transducer
// part its package ships (its manifest's `translate.transduce`: the
// alchemy file whose entry makes the format's JSON events of its parser's
// rule passes, through which `translate` reads the format's documents),
// where it ships one.
export class Format {
  // The transducer part compiled on its own, once asked for; null where
  // the package ships none.
  private compiled?: Program | null

  constructor(
    readonly part: translate.Part,
    private readonly reading: Reading,
    readonly transducer?: translate.Alc,
  ) {}

  // The manifest's `languageId`.
  get id(): string {
    return this.part.id
  }

  // Whether the format's documents are parsed, so that a program over
  // parser events can read them (`passes`); a format read as the tree its
  // package's API makes of a document has no parser here.
  parses(): boolean {
    const reading = this.reading
    return 'function' === typeof reading || 'parser' in reading
  }

  // The parser's rule passes over `text`: transduce's source of
  // `ParserEvents/1` with a fresh parser of the format's grammar, for a
  // program over parser events, a transducer part's or `run --grammar`'s.
  // The check the package's parse makes first runs in the parser's own
  // parse where the parser makes it (json5's), its refusal the run's, and
  // first otherwise (proto's), its refusal thrown here, as the package's
  // API reports it (`refusal`); a format read as a tree has no parser, and
  // refuses.
  passes(text: string): ParserEventSource {
    const reading = this.reading
    if ('function' === typeof reading) return new ParserEventSource(reading(), text)
    if (!('parser' in reading)) throw this.noPasses()
    checked(reading.check, text)
    return new ParserEventSource(reading.parser(), text)
  }

  // The parser's rule passes over a document read through `reader`, a
  // window of it at a time where the grammar declares `lex.window`; a read
  // that fails, or bytes that are not UTF-8, fail the run where the parse
  // meets them, with the engine's `unreadable` (`unreadable`). The check
  // the package's parse makes first runs in the parser's own `parseReader`
  // where the parser makes it (json5's), which reads only as far as the
  // check needs before the parse reads on; a check made first otherwise
  // (proto's nesting cap, which counts the whole document) reads the
  // document whole, and the parser then reads the text it passed.
  passesFrom(reader: Reader): ParserEventSource {
    const reading = this.reading
    if ('function' === typeof reading) return ParserEventSource.fromReader(reading(), reader)
    if (!('parser' in reading)) throw this.noPasses()
    const text = readAll(reader)
    checked(reading.check, text)
    return new ParserEventSource(reading.parser(), text)
  }

  // The rule passes over a document, from its text, or through a reader
  // over its file or its stream. A file that cannot be opened fails as one
  // that cannot be read (`cannotRead`); `close` closes what was opened.
  passesOf(document: Document): { source: ParserEventSource; close: () => void } {
    if ('text' in document) return { source: this.passes(document.text), close: () => {} }
    if ('reader' in document) return { source: this.passesFrom(document.reader), close: () => {} }
    let fd: number
    try {
      fd = openSync(document.file, 'r')
    } catch (err: any) {
      throw cannotRead(err?.message ?? String(err))
    }
    try {
      return { source: this.passesFrom(fileReader(fd)), close: () => closeSync(fd) }
    } catch (err) {
      closeSync(fd)
      throw err
    }
  }

  // `source`, pruned where transduce verifies the format's grammar alike
  // pruned and not (`isPrunable`, by the format's id): the engine's arrays
  // are emptied behind the stream, so its tree holds no record the stream
  // has passed. Any other grammar's source is left as it is, since a run
  // that prunes an unlisted grammar is refused before the parse.
  pruned(source: ParserEventSource): ParserEventSource {
    return isPrunable(this.id) ? source.grammar(this.id).prune() : source
  }

  // The refusal of a format read as a tree, which has no rule passes.
  private noPasses(): Fail {
    return Fail.input(`${this.id} is read as a tree, not parsed: it has no rule passes for a program to read`)
  }

  // A source over `text`, read whole as this format reads a document: its
  // parser's value, or the tree its package's API reads a document as,
  // where it has one.
  reader(text: string): DocumentSource {
    const reading = this.reading
    if ('function' === typeof reading) return new ParserSource(reading(), text)
    return new TreeSource(() => reading.tree(text))
  }

  // The transducer part compiled on its own, over the parser's rule
  // passes, its entry the export: the stage a host puts in front of a
  // program of its own, as `run` does in front of a program over a JSON
  // document, and the one `read` reads through. Compiled once, when first
  // asked for; undefined where the package ships no part.
  transducerProgram(): Program | undefined {
    if (undefined === this.compiled) {
      const alc = this.transducer
      this.compiled =
        undefined === alc
          ? null
          : compileSources(
              [
                { file: alc.file, text: alc.text },
                { file: 'main', text: `def export [input] (${alc.entry} input)` },
              ],
              { ...OPTIONS, input: 'ParserEvents/1' },
            )
    }
    return this.compiled ?? undefined
  }

  // A document read whole with this format's grammar, as a value: its
  // events as `translate` reads them, through the transducer part its
  // package ships (so that a number keeps the lexeme the document spelled
  // it with), collected; or the grammar's own value, walked, where the
  // package ships no part and where the part's events are not what the
  // grammar's value holds (a repeated member, which the part streams
  // twice). A repeated member keeps its last value.
  read(input: string, limits: Limits): Datum {
    const part = this.transducerProgram()
    if (undefined !== part) {
      try {
        return readThrough(this, part, input, limits)
      } catch (err) {
        if (!unfollowed(err)) throw err
      }
    }
    return read(this, input, limits)
  }
}

// A package's parts as alchemy's descriptor, named by the package as the
// Rust crate names it (`tabnas-yaml`), which is the name its parts carry
// in diagnostics, with the transducer part the package hands over beside
// it.
function descriptor(pkg: string, parts: Parts | undefined): Described | undefined {
  if (undefined === parts) return undefined
  return {
    descriptor: { package: pkg, manifest: parts.manifest, lift: parts.lift, embed: parts.embed, render: parts.render },
    transduce: parts.transduce,
  }
}

// Every grammar package this command carries, with how its documents are
// read: each one's default parser, as the Rust crate's `make()` builds it
// (the relaxed grammars on jsonic's base), or, where its parser's value is
// not yet the tree its parts declare, the tree its own API reads a
// document as, and, for proto, which ships a transducer part, both, with
// the nesting check its parse makes first.
function packages(): Array<[Described | undefined, Reading]> {
  return [
    [descriptor('tabnas-abnf', abnfParts()), { tree: abnfTree }],
    [descriptor('tabnas-c', cParts()), () => new Tabnas().use(jsonic).use(C)],
    [descriptor('tabnas-chess', chessParts()), () => makeChess()],
    [descriptor('tabnas-css', cssParts()), () => makeCss()],
    [descriptor('tabnas-csv', csvParts()), () => makeCsv()],
    [descriptor('tabnas-ebnf', ebnfParts()), { tree: ebnfTree }],
    [descriptor('tabnas-expr', exprParts()), { tree: exprTree }],
    [descriptor('tabnas-feed', feedParts()), () => makeFeed()],
    [descriptor('tabnas-gbnf', gbnfParts()), { tree: gbnfTree }],
    [descriptor('tabnas-ini', iniParts()), () => makeIni()],
    [descriptor('tabnas-json', jsonParts()), () => makeJson()],
    [descriptor('tabnas-json5', json5Parts()), () => makeJson5()],
    [descriptor('tabnas-jsonc', jsoncParts()), () => makeJsonc()],
    [descriptor('tabnas-jsonic', jsonicParts()), () => new Tabnas().use(jsonic)],
    [descriptor('tabnas-jsonl', jsonlParts()), () => makeJsonl()],
    [descriptor('tabnas-markdown', markdownParts()), () => makeMarkdown()],
    [descriptor('tabnas-proto', protoParts()), { parser: () => makeProto(), check: protoPreflight, tree: protoTree }],
    [descriptor('tabnas-semver', semverParts()), () => new Tabnas().use(Semver)],
    [descriptor('tabnas-toml', tomlParts()), () => makeToml()],
    [descriptor('tabnas-xml', xmlParts()), () => makeXml()],
    [descriptor('tabnas-yaml', yamlParts()), () => makeYaml()],
    [descriptor('tabnas-zon', zonParts()), () => zonParser()],
  ]
}

// What a package's API throws for a document it refuses, as the input's
// failure: the engine's error as the ParserSource reports one, with its
// code and position, and a refusal of the package's own (one with no
// engine code) with the package's message.
function refusal(err: unknown): unknown {
  if (isFail(err) || 'string' === typeof (err as { code?: unknown })?.code) return err
  return Fail.input(String((err as { message?: unknown })?.message ?? err))
}

// A check a package's own parse makes before its parser runs, over `text`:
// a document it refuses is not parsed, and the refusal is the input's
// failure, as the package's API reports it (`refusal`).
function checked(check: (text: string) => void, text: string): void {
  try {
    check(text)
  } catch (err) {
    throw engineFailure(refusal(err), new AbortFlag())
  }
}

// The tree a package's API reads a document as, or the refusal it throws.
function treeOf(read: () => unknown): unknown {
  try {
    return read()
  } catch (err) {
    throw refusal(err)
  }
}

// An expression as expr's API reads one for its shared fixtures, the tree
// its parts declare: each operation a list whose first element is the
// operator's source text (parseSimplified). The parser's own value holds
// each operator as the object that describes it, token and all.
function exprTree(text: string): unknown {
  return treeOf(() => parseSimplified(new Tabnas().use(jsonic).use(Expr), text))
}

// A .proto file read whole as proto's package reads one, the tree its
// parts declare: its declarations in source order (parseTree, which makes
// the check the parser does not, `preflight`: a document nesting deeper
// than its cap is refused before the engine runs). The parser's own value
// is the grammar's syntax tree.
function protoTree(text: string): unknown {
  return treeOf(() => protoParseTree(text))
}

// A grammar notation's document as its package's compiler writes it for a
// host, the tree its parts declare: the pure-data grammar spec, with its
// tree builders, as strict JSON, parsed as JSON. A document the compiler
// refuses is the input's failure, with the compiler's message, as the Rust
// and Go commands report it.
function specTree(compile: () => string): unknown {
  let text: string
  try {
    text = compile()
  } catch (err) {
    throw Fail.input(String((err as { message?: unknown })?.message ?? err))
  }
  return JSON.parse(text)
}

// An ABNF document: abnfCompile, recognition off, so that the tree builders
// stay. The grammar spec is the schema grammar-spec, which the EBNF and
// GBNF compilers emit too.
function abnfTree(text: string): unknown {
  return specTree(() => abnfCompile(text, { recognition: false, strict: true }))
}

// An EBNF document: the shared compiler's text of its conversion with the
// builtins on (compileSpec with recognition off, which is toJsonic over
// toPureSpec; @tabnas/abnf exports @tabnas/bnf's two).
function ebnfTree(text: string): unknown {
  return specTree(() => toJsonic(toPureSpec(ebnfConvert(text, { builtins: true })), { strict: true }))
}

// A GBNF document, as an EBNF one, with the notation's own defaults
// otherwise (the start root, eager classes where they are unambiguous).
function gbnfTree(text: string): unknown {
  return specTree(() => toJsonic(toPureSpec(gbnfConvert(text, { builtins: true })), { strict: true }))
}

// ZON's parser as its package's `make()` builds it, with an integer no
// double holds exactly as the object `{"$big": "<digits>"}`, the digits
// after a minus sign when it is negative: the reader's big integer as ZON's
// translation part declares it and its Rust reader builds it. The
// TypeScript reader builds a bigint, which transduce writes as the nearest
// double, so the token's value is replaced as it is read, before a rule
// takes it, and the value and the events hold the object.
function zonParser(): Tabnas {
  return makeZon().sub({
    lex: (tkn: { val: unknown }) => {
      if ('bigint' === typeof tkn.val) tkn.val = { $big: tkn.val.toString() }
    },
  })
}

let registry: ReadonlyArray<Format> | undefined

// Every format, by id, read once.
export function formats(): ReadonlyArray<Format> {
  if (undefined === registry) {
    const found: Format[] = []
    for (const [d, reading] of packages()) {
      const part = undefined === d ? undefined : translate.Part.fromDescriptor(d.descriptor)
      if (undefined === d || undefined === part) continue
      // A transducer the manifest and the package disagree on is none
      // here; the translation-parts test holds the two together.
      const transducer = translate.Part.transducer(d.descriptor, d.transduce)?.alc
      found.push(new Format(part, reading, transducer))
    }
    found.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    registry = Object.freeze(found)
  }
  return registry
}

// The format a command line names, by its id.
export function format(id: string): Format | undefined {
  return formats().find((f) => f.id === id)
}

// The formats' ids for a message: `csv, ini, ... or zon`.
export function names(): string {
  return namesOf(formats().map((f) => f.id))
}

// The ids of the formats whose documents are parsed, for a message, as
// `names` lists every format: what `run --grammar` takes.
export function parserNames(): string {
  return namesOf(formats().filter((f) => f.parses()).map((f) => f.id))
}

function namesOf(ids: ReadonlyArray<string>): string {
  if (ids.length < 2) return ids.join('')
  return `${ids.slice(0, -1).join(', ')} or ${ids[ids.length - 1]}`
}

// What a translation was asked.
export type Request = {
  // The format the input is read with.
  readonly from: Format
  // The format it is written in.
  readonly to: Format
  // A value below the root to translate instead of the whole document, as
  // segments: a plain tree, whatever the source's shapes.
  readonly path?: ReadonlyArray<Segment>
  // What the host chooses: the key a root is wrapped under.
  readonly options: translate.Options
  // A program, as its file name and its text, whose export stands in the
  // source's place: its JSON events a tree, its table records.
  readonly program?: { readonly file: string; readonly text: string }
  // Whether the document is read whole, as its grammar's value, rather
  // than streamed through its format's transducer part (`--whole`): the
  // route for a document the part refuses part way.
  readonly whole?: boolean
  readonly limits: Limits
}

// A request's composition, and the program it compiles to: the
// composition over the source's events, the route every format has, and,
// where the source ships a transducer part and the whole document is
// asked, not read whole, the composition over its parser's rule passes,
// which `runCompiled` then runs in its place.
export type Compiled = {
  readonly composition: translate.Composition
  readonly program: Program
  readonly parsed?: { readonly composition: translate.Composition; readonly program: Program }
}

// The composition a request makes, and the program it compiles to.
export function compile(request: Request): Compiled {
  // The transducer the route reads the document through, where the source
  // ships one and the whole document is asked, not read whole. A path's
  // value is a plain tree's events, which the composition over a plain
  // tree reads, and `runPath` reads the document through the part on its
  // own (`transducerProgram`) to select them.
  const transducer = undefined === request.path && !request.whole ? request.from.transducer : undefined
  const given = request.program
  if (undefined !== given) {
    const { file, text } = given
    const output = compileProgram(text, file, OPTIONS).output
    if ('Text' === output) {
      throw new Fail(
        'DSL_TYPE_ERROR',
        `bad_output: ${file}'s export writes its own text, and translate takes a program whose ` +
          "export answers JSON events or a table, which a format's render then writes",
      )
    }
    const composition = translate.composeProgram(output, request.to.part, request.options, 'translate')
    const program = composition.compile({ file, text }, OPTIONS)
    if (undefined === transducer) return { composition, program }
    const over = translate.composeProgramParsed(output, transducer, request.to.part, request.options, 'translate')
    return { composition, program, parsed: { composition: over, program: over.compile({ file, text }, OPTIONS) } }
  }
  const source = undefined === request.path ? request.from.part : undefined
  const composition = translate.compose(source, request.to.part, request.options, 'translate')
  const program = composition.compile(undefined, OPTIONS)
  if (undefined === transducer) return { composition, program }
  const over = translate.composeParsed(request.from.part, transducer, request.to.part, request.options, 'translate')
  return { composition, program, parsed: { composition: over, program: over.compile(undefined, OPTIONS) } }
}

// Whether a transducer part's route refused a document because the
// grammar's value is not what the passes showed: the part's own refusal
// (`fail :unstreamable`), a second root, or a repeated member, which the
// part streams twice and the composition's policy refuses, since the
// grammar's value may keep the last or merge the two. The output streams
// as the run goes, so such a document is not read again: it fails with its
// code, and `--whole` (the request's `whole`) reads it whole, as its
// grammar's value (admin ADR-33).
function unfollowed(err: unknown): err is Fail {
  return isFail(err) && ('STREAMABILITY_UNKNOWN' === err.code || 'DUPLICATE_MEMBER' === err.code)
}

// What a refusal of the transducer part's route adds to its message, where
// no program stands in the source's place, whose own events could make
// such a refusal: the route that reads the document.
const WHOLE = '; --whole reads the document whole'

// Run a request over `input`, writing the document to `out` (render's
// `Writer`, the counterpart of Rust's `io::Write`) as the run goes;
// `metrics` collects what the stages report, the output's bytes among it.
export function run(request: Request, input: string, out: Writer, metrics: Metrics): void {
  runCompiled(request, compile(request), input, out, metrics)
}

// `run`, with the request's composition compiled already (`compile`), so
// that one compiled composition serves many inputs. A failure after bytes
// have left says so (`committedOutput`): the source's failure (bad input
// after the rows) knows nothing of the output, and the writer's count does.
export function runCompiled(
  request: Request,
  compiled: Compiled,
  input: string,
  out: Writer,
  metrics: Metrics,
): void {
  runDocument(request, compiled, { text: input }, out, metrics)
}

// `runCompiled` over a document, which may be a file or a stream
// (`Document`). A document that cannot be read fails as one a reader gives
// up on part way does, with the engine's `unreadable` (`unreadable`).
export function runDocument(
  request: Request,
  compiled: Compiled,
  document: Document,
  out: Writer,
  metrics: Metrics,
): void {
  const { composition, program, parsed } = compiled
  try {
    if (undefined !== parsed) {
      // The transducer's route, where the source ships a part and the
      // whole document is asked: its rule passes through the part into the
      // program, streamed whatever the grammar, from a file or a stream
      // through a reader.
      const sink = parsed.program.parserSink(out, undefined, request.limits, metrics)
      const { source, close } = request.from.passesOf(document)
      try {
        request.from.pruned(source).limits(request.limits).metrics(metrics).run(sink)
      } catch (err) {
        if (unfollowed(err) && undefined === request.program) err.message += WHOLE
        throw err
      } finally {
        close()
      }
      return
    }
    if (undefined !== request.path) {
      runPath(request, program, request.path, document, out, metrics)
      return
    }
    const inner = program.sink(out, undefined, request.limits, metrics)
    // The source's events reach a tree's render as the source made them,
    // so a stream that is no tree's is refused in front of it rather than
    // written half way.
    const sink: Sink = 'tree' === composition.front ? new TreeContract(inner) : inner
    request.from.reader(textOf(document)).limits(request.limits).metrics(metrics).run(sink)
  } catch (err) {
    if (isFail(err) && metrics.output_bytes > 0 && !err.committedOutput) err.committed()
    throw err
  }
}

// The value a path names, translated as it streams (admin ADR-33): the
// document is read as the root is, through its format's transducer part
// on its own (`transducerProgram`) where the package ships one and the
// request does not ask it whole, from a file or a stream through a reader,
// and otherwise its grammar's value is walked; `AtPath` passes on the
// events of the value and drops every other. The document's events are
// held to a tree's contract in front, as the route of the whole document
// holds them, so the value is the one the grammar's value holds at the
// path or the run fails: a repeated member, which the part streams twice
// where the grammar's value keeps one, and a second root, a YAML stream's
// next document, are refused with their codes, and `--whole` reads such a
// document whole.
function runPath(
  request: Request,
  program: Program,
  path: ReadonlyArray<Segment>,
  document: Document,
  out: Writer,
  metrics: Metrics,
): void {
  const scope = new TreeContract(new AtPath(path, program.sink(out, undefined, request.limits, metrics)))
  const part = request.whole ? undefined : request.from.transducerProgram()
  if (undefined === part) {
    request.from.reader(textOf(document)).limits(request.limits).metrics(metrics).run(scope)
    return
  }
  const head = part.parserSinkInto(scope, request.limits, metrics)
  const { source, close } = request.from.passesOf(document)
  try {
    request.from.pruned(source).limits(request.limits).metrics(metrics).run(head)
  } catch (err) {
    if (unfollowed(err) && undefined === request.program) err.message += WHOLE
    throw err
  } finally {
    close()
  }
}

// One open container on the way to a path's value: an array, with the
// index of its next element, or an object, with the key of the member
// last named in it; and whether it is the value at the path's first
// segments, as many as the containers open around it (`onPath`).
type Around = { array: boolean; next: number; key: string | null; onPath: boolean }

// The events of the value a path names below a document's root, as a
// document of their own: every event outside the value is dropped, and the
// value's own are passed on as they come. The stream ends where the
// document does, which is read to its end, so a failure after the value is
// still the document's, and a path that names nothing fails there, with
// nothing written. The events are a tree's (a `TreeContract` holds them to
// it in front), so a value has one place and the path names at most one.
class AtPath implements Sink {
  private readonly around: Around[] = []
  // How deep the value is open while it is passed on, else 0.
  private depth = 0
  private found = false

  constructor(
    private readonly path: ReadonlyArray<Segment>,
    private readonly inner: Sink,
  ) {}

  event(ev: JsonEvent): Flow {
    if (0 < this.depth) {
      if ('object_start' === ev.type || 'array_start' === ev.type) this.depth++
      else if ('object_end' === ev.type || 'array_end' === ev.type) this.depth--
      if (0 === this.depth) this.found = true
      return this.inner.event(ev)
    }
    switch (ev.type) {
      case 'key': {
        const top = this.around[this.around.length - 1]
        if (undefined !== top) top.key = ev.key
        return 'continue'
      }
      case 'object_end':
      case 'array_end':
        this.around.pop()
        return 'continue'
      case 'end':
        if (!this.found) throw Fail.input(`the path ${pathText(this.path)} names nothing in the document`)
        return this.inner.event(ev)
      default: {
        // A value starts: the root, an element or a member's value.
        const onPath = this.onPath()
        const container = 'object_start' === ev.type || 'array_start' === ev.type
        if (onPath && this.around.length === this.path.length) {
          if (container) this.depth = 1
          else this.found = true
          return this.inner.event(ev)
        }
        if (container) {
          this.around.push({ array: 'array_start' === ev.type, next: 0, key: null, onPath })
        }
        return 'continue'
      }
    }
  }

  // Whether the value starting now is at the path's first segments, as
  // many as the containers open around it; an element counts as its
  // array's next.
  private onPath(): boolean {
    const level = this.around.length
    const top = this.around[level - 1]
    if (undefined === top) return true
    const at = top.array ? top.next++ : top.key
    if (!top.onPath || level > this.path.length) return false
    return at === this.path[level - 1]
  }
}

// The events of a document, collected into a value, which keeps the last
// of a repeated member.
class Collect implements Sink {
  private readonly builder = new DatumBuilder(Number.POSITIVE_INFINITY, 'max_capture_bytes', 'last_wins')

  event(ev: JsonEvent): 'continue' {
    if ('end' !== ev.type) this.builder.event(ev)
    return 'continue'
  }

  // The value collected, once the document has ended.
  take(): Datum {
    const value = this.builder.take()
    if (undefined === value) throw Fail.input('the document holds no value')
    return value
  }
}

// A document read whole with a format's grammar, as a value. A repeated
// member keeps its last value, as the grammar's own value does.
function read(format: Format, input: string, limits: Limits): Datum {
  const collect = new Collect()
  format.reader(input).limits(limits).run(collect)
  return collect.take()
}

// A document read through a format's transducer part, as a value: the
// part's events over the parser's rule passes, held to a tree's contract
// (one root, each key once in an object) and collected.
function readThrough(format: Format, part: Program, input: string, limits: Limits): Datum {
  const collect = new Collect()
  const metrics = new Metrics()
  const head = part.parserSinkInto(new TreeContract(collect), limits, metrics)
  format.passes(input).limits(limits).metrics(metrics).run(head)
  return collect.take()
}

// The largest index a path holds: Rust's `usize::MAX`, which a double
// reaches as 2^64.
const INDEX_MAX = 2 ** 64

// A path given as a JSON array of segments, each a string (a member's key)
// or a whole number (an element's index), the form alchemy's `as-path`
// validates.
export function parsePath(text: string, limits: Limits): Segment[] {
  const json = format('json')
  if (undefined === json) throw Fail.input('no JSON grammar to read the path')
  const refuse = () =>
    Fail.input(`--path takes a JSON array of keys and indexes, such as ["people",0], not ${text}`)
  let value: Datum
  try {
    value = json.read(text, limits)
  } catch (_err) {
    throw refuse()
  }
  if ('array' !== value.type) throw refuse()
  return value.items.map((item): Segment => {
    if ('string' === item.type) return item.value
    if ('number' === item.type && Number.isInteger(item.value) && item.value >= 0 && item.value <= INDEX_MAX) {
      // `-0` is the index 0, as Rust's cast makes it.
      return 0 === item.value ? 0 : item.value
    }
    throw refuse()
  })
}

// An index's digits, as Rust writes the `usize` it was cast to: exactly,
// and 2^64 as `usize::MAX`.
function indexText(index: number): string {
  return index >= INDEX_MAX ? (2n ** 64n - 1n).toString() : BigInt(index).toString()
}

// A path as the JSON array it was given as.
function pathText(path: ReadonlyArray<Segment>): string {
  return toText(
    Datum.array(path.map((s) => ('string' === typeof s ? Datum.string(s) : Datum.number(s, indexText(s))))),
  )
}

// The registry as JSON, one object per format, for `dmog formats`: its
// id, the shapes it reads and writes, the root its render needs, its
// schema, why its documents are read whole where it says (`whole`, null
// where it does not), its parts' entries, and its loss sentences.
export function formatsJson(): string {
  const text = (s: string): Datum => Datum.string(s)
  const orNull = (s: string | undefined): Datum => (undefined === s ? Datum.null : text(s))
  const items = formats().map((f) => {
    const p = f.part
    const render = p.render
    return Datum.object([
      ['id', text(p.id)],
      ['reads', Datum.array(p.reads.map(text))],
      ['writes', text(p.writes)],
      ['root', text(p.root)],
      ['schema', orNull(p.schema)],
      ['whole', orNull(p.whole)],
      ['lift', orNull(p.lift?.entry)],
      ['embed', orNull(p.embed?.entry)],
      ['render', text('alc' === render.kind ? render.alc.entry : render.kind)],
      ['transduce', orNull(f.transducer?.entry)],
      ['loss', Datum.array(p.loss.map(text))],
    ])
  })
  return toText(Datum.array(items))
}
