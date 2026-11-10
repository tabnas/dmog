/* Copyright (c) 2026 tabnas, MIT License */

// A format whose package ships a transducer part is read through it (the
// Rust crate's unit tests in rs/src/translate.rs): the route over its
// parser's rule passes, composed over the part, and what it falls back to.
// json's package ships one.

import { describe, it } from 'node:test'
import assert from 'node:assert'

import { isFail, translate as alchemyTranslate } from '@tabnas/alchemy'
import { make as makeJson } from '@tabnas/json'
import { BytesWriter } from '@tabnas/render'
import { Limits, Metrics } from '@tabnas/transduce'

import { Format, format as formatNamed, formats, parsePath, parserNames, run } from '../dist/translate'

// A transducer that writes `{"ran":true}` whatever the document: what it
// writes says which route ran.
const MARKING_TRANSDUCE =
  'def json-transduce [input]\n' +
  '  as-events\n' +
  '    scan-emit\n' +
  '      []\n' +
  '      fn [s pass] (transition s [])\n' +
  '      fn [s] [object-start (key "ran") (scalar true) object-end]\n' +
  '      parser-events input\n'

// json, read through a transducer part given in its package's place.
function transducedJson(text: string): Format {
  const json = formatNamed('json')
  assert.ok(json)
  return new Format(json.part, () => makeJson(), {
    file: 'tabnas-json/alchemy/transduce.alc',
    entry: 'json-transduce',
    text,
  })
}

// json without its transducer: read as a format whose package ships none,
// the walk over its parsed value.
function plainJson(): Format {
  const json = formatNamed('json')
  assert.ok(json)
  return new Format(json.part, () => makeJson())
}

function translated(
  from: Format,
  to: string,
  path: string | undefined,
  program: { file: string; text: string } | undefined,
  input: string,
  whole = false,
): string {
  const limits = Limits.default()
  const target = formatNamed(to)
  assert.ok(target)
  const held = new BytesWriter()
  run(
    {
      from,
      to: target,
      path: undefined === path ? undefined : parsePath(path, limits),
      options: alchemyTranslate.Options.default(),
      program,
      whole,
      limits,
    },
    input,
    held,
    new Metrics(),
  )
  return held.text()
}

describe('a transducer part', () => {
  // A format whose package ships a transducer part is read through it, its
  // rule passes streamed into the composition over the part, and one
  // without is read whole, its value walked: the two routes write the same
  // bytes for the same document, into a tree target, a records target, and
  // with a program over the input, but for a number's lexeme, which the
  // part keeps and the walk spells as the engine does; the part's own
  // output shows the route ran. A path selects a sub-stream of the same
  // events, so its value keeps its lexemes too, and read whole it is
  // selected in the grammar's value. A repeated member, which the part
  // streams twice and the composition's tree-contract refuses, fails with
  // its code and names --whole, since the output streams and is not
  // written twice, under a path as at the root; read whole, it is the
  // grammar's value, as a format without a part writes it (admin ADR-33).
  it('reads the format, and the walk stands in where it is asked', () => {
    const plain = plainJson()
    const through = formatNamed('json')
    assert.ok(through)
    const marking = transducedJson(MARKING_TRANSDUCE)
    assert.equal(plain.transducer, undefined)
    assert.equal(through.transducer?.entry, 'json-transduce')
    assert.ok(through.transducer?.text.includes('transduce-tree'))
    const tree = '{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}'
    // The same document, its numbers spelled as the engine spells them.
    const spelled = tree.replace('2.50', '2.5')
    assert.equal(translated(through, 'json', undefined, undefined, tree), `${tree}\n`, "the part keeps a number's lexeme")
    assert.equal(
      translated(plain, 'json', undefined, undefined, tree),
      `${spelled}\n`,
      'the walk spells a number as the engine does',
    )
    const records = '[{"a":1,"b":"x"},{"a":2,"b":"y"}]'
    const echo = { file: 'echo.alc', text: 'def export [input] (events input)\n' }
    const cases: Array<[string, string, { file: string; text: string } | undefined]> = [
      ['json', spelled, undefined],
      ['yaml', spelled, undefined],
      ['csv', records, undefined],
      ['json', spelled, echo],
      ['csv', records, echo],
    ]
    for (const [to, doc, program] of cases) {
      const expected = translated(plain, to, undefined, program, doc)
      assert.equal(translated(through, to, undefined, program, doc), expected, `${to} from ${doc}`)
      let marked: string | undefined
      try {
        marked = translated(marking, to, undefined, program, doc)
      } catch {
        marked = undefined
      }
      assert.notEqual(marked, expected, `${to} from ${doc}: the part's route ran`)
    }
    assert.equal(translated(marking, 'json', undefined, undefined, tree), '{"ran":true}\n')
    // A path selects a sub-stream of the part's events: its value keeps the
    // lexeme the document spelled a number with, and the marking part's own
    // events are what it selects in. Read whole, or without a part, a path
    // selects in the grammar's value, which spells a number as the engine
    // does; a path that names nothing fails at the document's end.
    assert.equal(translated(through, 'json', '["a",1]', undefined, tree), '2.50\n')
    assert.equal(translated(through, 'json', '["a"]', undefined, tree), '[1,2.50,"x",{"b":null}]\n')
    assert.equal(translated(through, 'json', '["a",3,"b"]', undefined, tree), 'null\n')
    assert.equal(translated(through, 'json', '[]', undefined, tree), `${tree}\n`)
    assert.equal(translated(marking, 'json', '["ran"]', undefined, tree), 'true\n')
    assert.equal(translated(through, 'json', '["a",1]', undefined, tree, true), '2.5\n')
    assert.equal(translated(plain, 'json', '["a",1]', undefined, tree), '2.5\n')
    assert.equal(translated(through, 'csv', '["rows"]', echo, `{"rows":${records}}`), translated(plain, 'csv', undefined, echo, records))
    for (const [from, path] of [
      [through, '["a",4]'],
      [through, '["a","0"]'],
      [through, '["c",0]'],
      [through, '["f","g"]'],
      [plain, '["z"]'],
      [marking, '["a"]'],
    ] as Array<[Format, string]>) {
      assert.throws(
        () => translated(from, 'json', path, undefined, tree),
        (err: unknown) =>
          isFail(err) &&
          'INPUT_INVALID' === err.code &&
          `the path ${path} names nothing in the document` === err.message &&
          !err.committedOutput,
        path,
      )
    }
    const repeated = '{"a":1,"a":2}'
    assert.throws(
      () => translated(through, 'json', undefined, undefined, repeated),
      (err: unknown) =>
        isFail(err) && 'DUPLICATE_MEMBER' === err.code && err.message.endsWith('; --whole reads the document whole'),
    )
    assert.equal(
      translated(through, 'json', undefined, undefined, repeated, true),
      translated(plain, 'json', undefined, undefined, repeated),
    )
    assert.equal(translated(through, 'json', undefined, undefined, repeated, true), '{"a":2}\n')
    // Under a path, a repeated member fails as it does at the root, inside
    // the value, on the way to it or past it, since the grammar's value
    // decides which of the two a path names; read whole, the path selects
    // in it.
    const around = '{"x":{"a":1,"a":2},"y":3}'
    for (const path of ['["x"]', '["x","a"]', '["y"]']) {
      assert.throws(
        () => translated(through, 'json', path, undefined, around),
        (err: unknown) =>
          isFail(err) && 'DUPLICATE_MEMBER' === err.code && err.message.endsWith('; --whole reads the document whole'),
        path,
      )
    }
    assert.equal(translated(through, 'json', '["x"]', undefined, around, true), '{"a":2}\n')
    assert.equal(translated(through, 'json', '["x","a"]', undefined, around, true), '2\n')
    assert.throws(
      () => translated(through, 'json', undefined, undefined, '[1,'),
      (err: unknown) => isFail(err) && 'INPUT_INVALID' === err.code,
    )
  })

  // YAML is read through its part, and a repeated key, which the part
  // streams twice and the tree contract refuses, fails unless the
  // document is read whole: the grammar merges it, at the root and in a
  // sequence entry's mapping alike (@tabnas/yaml 0.5.26 merges the
  // entry's, which kept the later value before). A sequence entry's keys
  // stand at the column its content starts at, so a key with an empty
  // value before a sibling is null, and an entry with more than one space
  // after its dash reads (both 0.5.26).
  it('reads a YAML sequence entry as its grammar does', () => {
    const yaml = formatNamed('yaml')
    assert.ok(yaml)
    const cases: Array<[string, string, boolean]> = [
      ['a:\n  b: 1\na:\n  c: 2\n', '{"a":{"b":1,"c":2}}\n', true],
      ['- a:\n    b: 1\n  a:\n    c: 2\n', '[{"a":{"b":1,"c":2}}]\n', true],
      ['- a:\n  b: 1\n', '[{"a":null,"b":1}]\n', false],
      ['-   a: 1\n    b: 2\n', '[{"a":1,"b":2}]\n', false],
    ]
    for (const [doc, want, repeats] of cases) {
      if (repeats) {
        assert.throws(
          () => translated(yaml, 'json', undefined, undefined, doc),
          (err: unknown) => isFail(err) && 'DUPLICATE_MEMBER' === err.code,
          JSON.stringify(doc),
        )
      }
      assert.equal(translated(yaml, 'json', undefined, undefined, doc, repeats), want, JSON.stringify(doc))
    }
  })

  // Every format with a parser answers its rule passes; one read as a tree
  // has none and refuses; json5's parser checks a document holds a value
  // in its own parse, so the run refuses one that holds none.
  it('every format with a parser gives its rule passes', () => {
    const parsed = formats()
      .filter((f) => f.parses())
      .map((f) => f.id)
    // C is parsed here, where the Rust crate reads it as its realized tree.
    assert.deepStrictEqual(parsed, [
      'abnf', 'c', 'css', 'csv', 'ebnf', 'feed', 'gbnf', 'ini', 'json', 'json5', 'jsonc', 'jsonic', 'jsonl',
      'markdown', 'pgn', 'proto', 'semver', 'toml', 'xml', 'yaml', 'zon',
    ])
    assert.equal(
      parserNames(),
      'abnf, c, css, csv, ebnf, feed, gbnf, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, proto, semver, ' +
        'toml, xml, yaml or zon',
    )
    const expr = formatNamed('expr')
    assert.ok(expr)
    assert.equal(expr.parses(), false)
    assert.throws(
      () => expr.passes('1+2'),
      (err: unknown) => isFail(err) && 'INPUT_INVALID' === err.code && err.message.startsWith('expr is read as a tree'),
    )
    const json5 = formatNamed('json5')
    assert.ok(json5)
    const none = json5.passes('// only a comment')
    assert.throws(
      () => none.run({ event: () => 'continue' }),
      (err: unknown) =>
        isFail(err) && 'INPUT_INVALID' === err.code && err.message.startsWith('json5_no_value: '),
    )
    let passes = 0
    json5.passes('{a: 1}').run({
      event: () => {
        passes += 1
        return 'continue'
      },
    })
    assert.ok(0 < passes)
  })
})
