/* Copyright (c) 2026 tabnas, MIT License */

// alchemy's shared fixtures (its test/spec, vendored into
// test/fixtures/alchemy) that need the stages, run through @tabnas/support's runner as
// alchemy runs the rest (rs/tests/spec_test.rs is the Rust half): the bytes
// a run writes (run.tsv), the bytes a run over a grammar's rule passes
// writes (transduce.tsv), and the catalogue's drift test over the error
// rows of all five files.
//
// run.tsv runs each row through the library's text, the one
// implementation of its compositions, and holds it to its expected cell.
//
// Moved from alchemy's ts/test/spec.test.ts, which runs reader.tsv,
// pipe.tsv and check.tsv and the catalogue's per-row checks over them:
// run.tsv's rows run programs on transduce's routers and render's
// renderers, which alchemy does not depend on, and the catalogue's
// coverage clause needs those rows.

import { describe, it } from 'node:test'
import assert from 'node:assert'
import { join } from 'node:path'

import { SpecRow, isErrorExpect, loadSpecDir, makeRunner } from '@tabnas/support'
import { BytesWriter } from '@tabnas/render'
import { DatumBuilder } from '@tabnas/transduce'
import type { Datum } from '@tabnas/transduce'

import { Format, format as formatNamed, parserNames } from '../dist/translate'

import {
  Program,
  Renderer,
  desugarProgram,
  grammarDocument,
  isFail,
  make as makeAlchemy,
  parse,
  rendererNamed,
} from '@tabnas/alchemy'
import { Limits, Metrics } from '@tabnas/alchemy/shared'
import type { Duplicates } from '@tabnas/alchemy/shared'

import { Collect, OWN_CODES, compile, compileWith, failCode, jsonInto, plainEvents, vendored } from './host'

// alchemy's fixtures, vendored into test/fixtures.
const SPEC_DIR = vendored('alchemy', 'test', 'spec')

// The finer code a row pins, and the position the failure names.
const runnerOptions = {
  errorCode: (err: unknown) => failCode(err),
  errorPos: (err: any) => ({ row: err?.row, col: err?.col }),
}

// Compile `program` and run it over `doc`, as `dmog run` does: the
// document read through JSON's transducer part, with the default limits.
function runProgram(program: string, doc: string, render: Renderer | undefined): string {
  return drive(compile(program, 'run'), doc, render)
}

function drive(program: Program, doc: string, render: Renderer | undefined): string {
  const limits = Limits.default()
  const metrics = new Metrics()
  const writer = new BytesWriter()
  const sink = program.sink(writer, render, limits, metrics)
  jsonInto(doc, limits, metrics, sink)
  return writer.text()
}

// A row's `render` column: how a table or JSON-events result is rendered,
// or the host's default.
function rowRender(row: SpecRow): Renderer | undefined {
  const name = row.named('render')
  if ('' === name) return undefined
  const render = rendererNamed(name)
  if (undefined === render) throw new Error(`${row.where()}: render is csv, json or empty`)
  return render
}

// A program run over a JSON document: the bytes it writes, or the failure.
// See test/AGENTS.md for the columns.
makeRunner({
  ...runnerOptions,
  parse: (program: string, row: SpecRow) => {
    const cell = row.unescNamed('doc')
    const doc = '' === cell ? 'null' : cell
    return runProgram(program, doc, rowRender(row))
  },
}).file(join(SPEC_DIR, 'run.tsv'))

// The failures a row's program meets: the reader's (reader.tsv), the
// desugarer's (pipe.tsv), compile's (check.tsv, over the input its
// `protocol` column declares: empty for JSON events, `parser` for the
// parser's rule passes), or a run's, both paths (run.tsv), as the runners
// above meet them.
function rowFailures(file: string, input: string, row: SpecRow): unknown[] {
  const met = (work: () => unknown): unknown[] => {
    try {
      work()
      return []
    } catch (fail) {
      return [fail]
    }
  }
  switch (file) {
    case 'reader.tsv':
      return met(() => parse(input))
    case 'pipe.tsv':
      return met(() => desugarProgram(parse(input), input))
    case 'check.tsv': {
      const protocol = row.named('protocol')
      if ('' !== protocol && 'parser' !== protocol) {
        throw new Error(`${row.where()}: the protocol is empty or parser, not ${JSON.stringify(protocol)}`)
      }
      return met(() => compileWith(input, 'check', 'parser' === protocol ? 'ParserEvents/1' : 'JsonEvents/1'))
    }
    case 'transduce.tsv': {
      const grammar = transduceGrammar(row)
      const doc = row.unescNamed('doc')
      const render = rowRender(row)
      const duplicates = rowDuplicates(row)
      return met(() => runPasses(input, grammar, doc, render, duplicates).text)
    }
    default: {
      const cell = row.unescNamed('doc')
      const doc = '' === cell ? 'null' : cell
      const render = rowRender(row)
      return met(() => runProgram(input, doc, render))
    }
  }
}

// Whether `text` is an instance of the template `line`. The engine trims
// the messages it writes from a template, so a `{name}` that ends a line
// may take the space before it with it.
function instanceOf(line: string, text: string): boolean {
  if (fills(line, text)) return true
  const bare = line.replace(/\s*\{[a-z_]+\}$/, '')
  return bare !== line && fills(bare, text)
}

// Whether `text` fills the template `line`: its fixed parts in order, each
// `{name}` any text, none included.
function fills(line: string, text: string): boolean {
  const parts = line.split(/\{[a-z_]+\}/)
  if (1 === parts.length) return text === line
  const first = parts[0]
  const last = parts[parts.length - 1]
  const end = text.length - last.length
  if (end < first.length || !text.startsWith(first) || !text.endsWith(last)) return false
  let at = first.length
  for (const part of parts.slice(1, -1)) {
    const found = text.indexOf(part, at)
    if (-1 === found || found + part.length > end) return false
    at = found + part.length
  }
  return true
}

// The fixed text of a template: what a more specific line has more of. A
// failure meets the most specific line it is an instance of, the first of
// equals.
function fixedLength(line: string): number {
  return line.replace(/\{[a-z_]+\}/g, '').length
}

// Every failure the shared fixtures meet is declared in the grammar
// document (rs/tests/spec_test.rs, `the_raised_messages_match_the_document`):
// the finer code that leads the message is a key of the installed
// `options.error`, the engine's and this grammar's, and the text after it is
// a line of that entry, each `{name}` standing for what the raising site
// fills in (a failure raised inside the standard library ends with its
// position there, ` (at stdlib/...)`, which is not part of the text). Each
// line of each code the document declares is the most specific line some
// row meets, so the document holds no text nothing raises, and each of its
// codes has a hint. A finer code comes with the same code wherever it is
// raised (`bad_let` is a DSL_PARSE_ERROR from the desugarer and from the
// resolver alike). A raising site whose code or text drifts fails here.
describe('the catalogue', () => {
  it('the raised messages match the document', () => {
    const catalogue: Record<string, string> = makeAlchemy().options.error
    const document = grammarDocument()
    const met = new Set<string>()
    // The codes each finer code is raised with: one, wherever it is raised.
    const raisedAs = new Map<string, Set<string>>()
    const problems: string[] = []
    let failures = 0
    for (const spec of loadSpecDir(SPEC_DIR)) {
      for (const row of spec.rows) {
        if (!isErrorExpect(row.col(1))) continue
        const input = row.unesc(row.resolve(0))
        for (const fail of rowFailures(spec.file, input, row)) {
          if (!isFail(fail) || !OWN_CODES.includes(fail.code)) continue
          failures++
          const split = fail.message.indexOf(': ')
          if (-1 === split) {
            problems.push(`${row.where()}: ${fail.code} has no finer code: ${fail.message}`)
            continue
          }
          const code = fail.message.substring(0, split)
          if (!raisedAs.has(code)) raisedAs.set(code, new Set())
          raisedAs.get(code)?.add(fail.code)
          let text = fail.message.substring(split + 2)
          const library = text.lastIndexOf(' (at stdlib/')
          if (-1 !== library && text.endsWith(')')) text = text.substring(0, library)
          const entry = catalogue[code]
          if ('string' !== typeof entry) {
            problems.push(`${row.where()}: ${code} is not declared in options.error`)
            continue
          }
          const lines = entry
            .split('\n')
            .filter((line) => instanceOf(line, text))
            .sort((a, b) => fixedLength(b) - fixedLength(a))
          if (0 === lines.length) {
            problems.push(`${row.where()}: no line of options.error.${code} is ${JSON.stringify(text)}`)
          } else {
            met.add(code + '\n' + lines[0])
          }
        }
      }
    }
    assert.ok(failures > 0, 'the fixtures meet failures')
    for (const [code, uppers] of raisedAs) {
      if (uppers.size > 1) problems.push(`${code} is raised as ${[...uppers].sort().join(' and ')}; a finer code has one code`)
    }
    for (const [code, entry] of Object.entries<string>(document.options.error)) {
      if ('string' !== typeof document.options.hint[code]) problems.push(`options.hint.${code} is not declared`)
      for (const line of entry.split('\n')) {
        if (!met.has(code + '\n' + line)) problems.push(`options.error.${code}: no fixture row meets ${JSON.stringify(line)}`)
      }
    }
    assert.deepStrictEqual(problems, [])
  })
})

// The policy a transduce row's `duplicates` names for a member name an
// object repeats within a captured value: empty is the default, first
// keeps the earlier value, last the later.
function rowDuplicates(row: SpecRow): Duplicates {
  const name = row.named('duplicates')
  if ('' === name) return 'reject'
  if ('first' === name) return 'first_wins'
  if ('last' === name) return 'last_wins'
  throw new Error(`${row.where()}: duplicates is empty, first or last, not ${JSON.stringify(name)}`)
}

// The format a transduce row's `grammar` names, whose parser reads its
// document: one of the command's own, with a parser.
function transduceGrammar(row: SpecRow): Format {
  const name = row.named('grammar')
  const grammar = formatNamed(name)
  if (undefined === grammar || !grammar.parses()) {
    throw new Error(
      `${row.where()}: grammar names a format whose documents are parsed (${parserNames()}), not ${JSON.stringify(name)}`,
    )
  }
  return grammar
}

// Compile `program` over the parser's rule passes and run it over `doc`,
// parsed with `grammar`, as `dmog run --grammar` does, with the default
// limits. The program's output kind rides along, for the differential.
function runPasses(
  program: string,
  grammar: Format,
  doc: string,
  render: Renderer | undefined,
  duplicates: Duplicates,
): { output: string; text: string } {
  let compiled = compileWith(program, 'transduce', 'ParserEvents/1')
  if ('reject' !== duplicates) compiled = compiled.withDuplicates(duplicates)
  const writer = new BytesWriter()
  const limits = Limits.default()
  const metrics = new Metrics()
  const sink = compiled.parserSink(writer, render, limits, metrics)
  grammar.passes(doc).limits(limits).metrics(metrics).run(sink)
  return { output: compiled.output, text: writer.text() }
}

// The differential rule of transduce.tsv: the events a row's program
// wrote, read back from its JSON, are the walk of the grammar's own value
// of the document, a number's lexeme aside. A document the grammar refuses
// has no value to walk, and a member the program streamed twice is kept
// once by the value: neither row has an oracle, and both pass here, held
// by their expected column alone.
function differential(grammar: Format, doc: string, written: string): void {
  let value: Datum
  try {
    value = grammar.read(doc, Limits.default())
  } catch {
    return
  }
  const collect = new Collect(new DatumBuilder(Number.POSITIVE_INFINITY, 'max_capture_bytes', 'reject'))
  try {
    jsonInto(written, Limits.default(), new Metrics(), collect)
  } catch (err) {
    if (isFail(err) && 'DUPLICATE_MEMBER' === err.code) return
    throw new Error(`the output ${JSON.stringify(written)} does not read back as JSON: ${String(err)}`)
  }
  const read = collect.builder.take()
  if (undefined === read) throw new Error(`the output ${JSON.stringify(written)} holds no value`)
  assert.deepStrictEqual(
    plainEvents(read),
    plainEvents(value),
    "the transducer's events are not the walk of the grammar's value",
  )
}

// A program over a grammar's rule passes, run over a document in that
// grammar: the bytes it writes, or the failure (alchemy's test/AGENTS.md
// has the columns). A row's program is a transducer, its events the
// document's, which the differential holds it to where it answers JSON
// events.
makeRunner({
  ...runnerOptions,
  parse: (program: string, row: SpecRow) => {
    const grammar = transduceGrammar(row)
    const doc = row.unescNamed('doc')
    const got = runPasses(program, grammar, doc, rowRender(row), rowDuplicates(row))
    if ('JsonEvents/1' === got.output) differential(grammar, doc, got.text)
    return got.text
  },
}).file(join(SPEC_DIR, 'transduce.tsv'))
