/* Copyright (c) 2026 tabnas, MIT License */

// The transducer part tabnas-abnf generates for an ABNF grammar (step 8
// of alchemy's `docs/streaming-plan.md`), run as a host runs it
// (rs/tests/generated_part_test.rs): compiled by alchemy, over the
// grammar's rule passes from transduce's `ParserEventSource`. Over every
// grammar of abnf's `alignment-abnf-ast.tsv`, a generated part writes the
// value the compiled grammar builds, and the generator declares every other grammar read whole, for the reason it
// gives for a start rule with no annotation. The part's events reach a
// renderer the program builds (`parserSink`) and a sink of the host's own
// (`parserSinkInto`), which is how aless runs a part in front of its own
// stages.

import { describe, it } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { abnf as abnfPlugin, abnfTransduce } from '@tabnas/abnf'
import { BytesWriter, renderers } from '@tabnas/render'
import { ParserEventSource } from '@tabnas/transduce'
import { loadSpec } from '@tabnas/support'
import { Program } from '@tabnas/alchemy'
import { Limits } from '@tabnas/alchemy/shared'

import { compileWith, vendored } from './host'

// An engine carrying the grammar, as abnf's own tests install one.
function engine(grammar: string): Tabnas {
  const tn: any = new Tabnas({ plugins: [abnfPlugin] })
  tn.abnf(grammar)
  return tn
}

// One grammar of the fixture whose generated part streams: a document,
// the value the grammar builds of it as JSON, and the part compiled.
interface Case {
  grammar: string
  input: string
  want: string
  program: Program
}

// Every grammar of abnf's `alignment-abnf-ast.tsv` with its generated
// part, each grammar the generator declares read whole held to the reason
// it gives for a start rule with no annotation, never an annotated one.
function cases(): Case[] {
  const fixture = loadSpec(vendored('abnf', 'test', 'spec', 'alignment-abnf-ast.tsv'))
  const out: Case[] = []
  for (const row of fixture.rows) {
    const grammar = row.unescNamed('grammar')
    const input = row.unescNamed('input')
    const annotated = grammar.includes('@object') || grammar.includes('@array')
    const part: any = abnfTransduce(grammar, 'g')
    if (undefined !== part.whole) {
      assert.ok(!annotated && part.whole.startsWith('the start rule '), `${JSON.stringify(grammar)} is read whole: ${part.whole}`)
      continue
    }
    const want = JSON.stringify(engine(grammar).parse(input))
    const program = compileWith(`${part.source}\ndef export [input] (${part.entry} input)\n`, 'generated.alc', 'ParserEvents/1')
    out.push({ grammar, input, want, program })
  }
  assert.ok(10 < out.length, `${out.length} grammars streamed`)
  return out
}

// What a run wrote as JSON, written again in one form, or its failure.
function written(work: () => string): string {
  try {
    return JSON.stringify(JSON.parse(work()))
  } catch (err) {
    return String(err)
  }
}

describe('generated parts', () => {
  it('a generated part writes the value its grammar builds', () => {
    const failures: string[] = []
    for (const c of cases()) {
      {
      const program = c.program
        const got = written(() => {
          const writer = new BytesWriter()
          new ParserEventSource(engine(c.grammar), c.input).run(program.parserSink(writer, 'json'))
          return writer.text()
        })
        if (got !== c.want) {
          failures.push(`${JSON.stringify(c.grammar)} on ${JSON.stringify(c.input)}\n  part:  ${got}\n  value: ${c.want}`)
        }
      }
    }
    assert.deepStrictEqual(failures, [])
  })

  // The parts again, their events into a sink of the host's own
  // (`parserSinkInto`), as aless runs a format's part in front of stages
  // of its own: here render's JSON renderer, which the host builds itself,
  // writes the value each grammar builds. The source's limits hold on the part's events, as on a source's own.
  it("a generated part's events reach a host's own sink", () => {
    const failures: string[] = []
    const all = cases()
    for (const c of all) {
      {
      const program = c.program
        const got = written(() => {
          const writer = new BytesWriter()
          const own = renderers.json(renderers.writeOut(writer))
          new ParserEventSource(engine(c.grammar), c.input).run(program.parserSinkInto(own))
          return writer.text()
        })
        if (got !== c.want) {
          failures.push(`${JSON.stringify(c.grammar)} on ${JSON.stringify(c.input)}\n  part:  ${got}\n  value: ${c.want}`)
        }
      }
    }
    assert.deepStrictEqual(failures, [])

    const c = all[0]
    const tight = { ...Limits.default(), max_depth: 0 }
    {
      const program = c.program
      const own = renderers.json(renderers.writeOut(new BytesWriter()))
      assert.throws(
        () => new ParserEventSource(engine(c.grammar), c.input).run(program.parserSinkInto(own, tight)),
        (err: any) => 'max_depth' === err?.limit?.name,
        "a part's events are held to the source's limits",
      )
    }
  })
})
