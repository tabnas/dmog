/* Copyright (c) 2026 tabnas, MIT License */

// What alchemy's tests that run programs share, as a host composes them:
// compile with transduce's routers and render's renderers, drive a program
// over a JSON text, record and replay a document's events, tell a
// failure's finer code, and find a vendored test file (alchemy's
// ts/test/common.ts and rs/tests/common/mod.rs, where these were defined
// first).
//
// Those tests moved here from alchemy, which depends on neither transduce
// nor render; this package is the composition root that depends on all
// three. They are black-box tests of alchemy's public API: they import its
// package entry (`@tabnas/alchemy`) and its shared unit
// (`@tabnas/alchemy/shared`), never its build's own modules.

import { existsSync } from 'node:fs'
import { join } from 'node:path'

import * as alchemy from '@tabnas/alchemy'
import { DatumBuilder, EventRecorder, Limits, Metrics, replay, walkDatum } from '@tabnas/alchemy/shared'
import type { Datum, Flow, JsonEvent, Sink } from '@tabnas/alchemy/shared'
import { BytesWriter, renderers } from '@tabnas/render'
import { ParserSource, routers } from '@tabnas/transduce'

import { format } from '../dist/translate'

// This repository's root: two levels above dist-test/.
export const REPO_ROOT = join(__dirname, '..', '..')

// Where the test files vendored from other tabnas repositories are, each
// at its path there: test/fixtures, which tools/vendor-fixtures.sh writes,
// or DMOG_FIXTURES, which the gates set to the directory of the sibling
// checkouts when they run against them (DMOG_SIBLINGS=1).
export const FIXTURES_ROOT = process.env.DMOG_FIXTURES || join(REPO_ROOT, 'test', 'fixtures')

// A path among the vendored test files (`parts[0]` names the repository),
// such as alchemy's fixtures (`vendored('alchemy', 'test', 'spec')`). A
// missing path fails the tests that read it, naming the path; nothing
// skips.
export function vendored(...parts: string[]): string {
  const path = join(FIXTURES_ROOT, ...parts)
  if (!existsSync(path)) {
    throw new Error(`${path} does not exist: run tools/vendor-fixtures.sh`)
  }
  return path
}

// What a host passes `compile`: transduce's routers and render's renderers,
// the stages a program's runtime builds a run from.
export const OPTIONS: alchemy.CompileOptions = { routers, renderers }

// `compile` as a host calls it, with `OPTIONS`.
export function compile(src: string, file: string): alchemy.Program {
  return alchemy.compile(src, file, OPTIONS)
}

// `compile` over the input the host declares: JSON events, or the parser's
// rule passes.
export function compileWith(src: string, file: string, input: alchemy.Input): alchemy.Program {
  return alchemy.compile(src, file, { ...OPTIONS, input })
}

// `compileSources` as a host calls it, with `OPTIONS`.
export function compileSources(sources: ReadonlyArray<alchemy.Source>): alchemy.Program {
  return alchemy.compileSources(sources, OPTIONS)
}

// The codes whose failures carry a finer code as the first word of the
// message, before `: `.
export const OWN_CODES = ['DSL_PARSE_ERROR', 'DSL_TYPE_ERROR', 'STREAM_REUSED', 'STREAMABILITY_UNKNOWN']

// The code a fixture pins for a failure: the finer code for alchemy's own
// codes, the transduce code itself for any other. A failure render made
// counts, whichever copy of transduce render resolves (`isFail`).
export function failCode(err: unknown): string | undefined {
  if (!alchemy.isFail(err)) return undefined
  if (OWN_CODES.includes(err.code)) {
    const at = err.message.indexOf(': ')
    return -1 === at ? err.message : err.message.substring(0, at)
  }
  return err.code
}

// The finer code of a failure that must be a `Fail` of one of alchemy's
// codes, or a description of what was thrown instead.
export function finer(err: unknown): string {
  return failCode(err) ?? `not a Fail: ${err}`
}

// What `fn` throws; fails the test when it returns.
export function thrown(fn: () => unknown): any {
  try {
    fn()
  } catch (err) {
    return err
  }
  throw new Error('expected a failure')
}

// Push the events of the JSON document `text` into `sink`, as `alchemy
// run` reads its input (rs/tests/common/mod.rs's `json_into`): JSON's own
// transducer part over the parser's rule passes, the engine's arrays
// emptied behind it.
export function jsonInto(text: string, limits: any, metrics: any, sink: any): void {
  const json = format('json')
  if (undefined === json) throw new Error('the command reads JSON')
  const part = json.transducerProgram()
  if (undefined === part) throw new Error('the JSON grammar ships a transducer part')
  const head = part.parserSinkInto(sink, limits, metrics)
  json.passes(text).grammar(json.id).prune().limits(limits).metrics(metrics).run(head)
}

// What a host does with a program and a JSON text (rs/tests/run_test.rs's
// `drive`): read the text as `dmog run` reads its input, push the
// events into the program's sink, and mark a failure as leaving partial
// output when `output_bytes` says bytes reached the writer. The outcome,
// and the text the writer received.
export function drive(
  program: any,
  text: string,
  render?: 'csv' | 'json',
  limits: any = Limits.default(),
  metrics: any = new Metrics(),
): { fail?: any; out: string } {
  const writer = new BytesWriter()
  let sink: any
  try {
    sink = program.sink(writer, render, limits, metrics)
  } catch (fail) {
    return { fail, out: '' }
  }
  try {
    jsonInto(text, limits, metrics, sink)
  } catch (fail) {
    if (alchemy.isFail(fail) && metrics.output_bytes > 0 && !fail.committedOutput) fail.committed()
    return { fail, out: writer.text() }
  }
  return { out: writer.text() }
}

// The output of a run that must succeed.
export function ok(program: any, text: string, render?: 'csv' | 'json'): string {
  const { fail, out } = drive(program, text, render)
  if (undefined !== fail) throw fail
  return out
}

// The failure of a run that must fail, and what had been written.
export function err(program: any, text: string, limits: any = Limits.default()): { fail: any; out: string } {
  const { fail, out } = drive(program, text, undefined, limits)
  if (undefined === fail) throw new Error(`the run should fail, and wrote ${JSON.stringify(out)}`)
  return { fail, out }
}

// The events of one document, recorded once so several runs replay the
// same stream: the walk of the grammar's value (materialize mode), sound
// for every grammar. Undefined when the grammar refuses the document.
export function events(parser: any, text: string): any[] | undefined {
  const recorder = new EventRecorder()
  try {
    new ParserSource(parser, text).run(recorder)
  } catch (_fail) {
    return undefined
  }
  return recorder.events
}

// Replay `events` through `program`'s sink; the output, or the failure
// thrown.
export function replayed(
  program: any,
  evs: any[],
  render?: 'csv' | 'json',
  limits: any = Limits.default(),
  metrics: any = new Metrics(),
): string {
  const writer = new BytesWriter()
  const sink = program.sink(writer, render, limits, metrics)
  replay(evs, sink)
  return writer.text()
}

// The events of a document, collected into a value under a duplicate
// policy.
export class Collect implements Sink {
  constructor(readonly builder: DatumBuilder) {}

  event(ev: JsonEvent): Flow {
    if ('end' !== ev.type) this.builder.event(ev)
    return 'continue'
  }
}

// A value's events, a number's lexeme dropped.
export function plainEvents(value: Datum): JsonEvent[] {
  const recorder = new EventRecorder()
  walkDatum(value, recorder)
  return recorder.events.map((ev) => ('number' === ev.type ? { ...ev, lexeme: null } : ev))
}
