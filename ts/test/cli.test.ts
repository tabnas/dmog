/* Copyright (c) 2026 tabnas, MIT License */

// The `dmog` command (rs/tests/cli_test.rs in Rust), run the way a
// script runs it: bin/dmog in a child process, the five commands of
// programs and the two of formats (`translate` and `formats`), standard
// input as `-`, the statuses, and that nothing but the answer reaches
// standard output.

import { describe, it } from 'node:test'
import assert from 'node:assert'
import { SpawnSyncReturns, spawn, spawnSync } from 'node:child_process'
import { accessSync, constants, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { translate as abnfParts } from '@tabnas/abnf'
import { source } from '@tabnas/alchemy'
import { translate as cParts } from '@tabnas/c'
import { translate as cssParts } from '@tabnas/css'
import { translate as ebnfParts } from '@tabnas/ebnf'
import { translate as exprParts } from '@tabnas/expr'
import { translate as feedParts } from '@tabnas/feed'
import { translate as gbnfParts } from '@tabnas/gbnf'
import { translate as iniParts } from '@tabnas/ini'
import { make as makeJson } from '@tabnas/json'
import { translate as semverParts } from '@tabnas/semver'
import { translate as tomlParts } from '@tabnas/toml'

import { EXPECTED_CSV, PROGRAM, RECORDS } from './common'

// The command, as the package's `bin` names it.
const BIN = join(__dirname, '..', 'bin', 'dmog')

const TMP = mkdtempSync(join(tmpdir(), 'dmog-'))

type Output = { status: number | null; stdout: string; stderr: string }

function dmog(args: string[], stdin?: string | Buffer, cwd?: string): Output {
  const out: SpawnSyncReturns<Buffer> = spawnSync(process.execPath, [BIN, ...args], {
    input: stdin,
    stdio: [undefined === stdin ? 'ignore' : 'pipe', 'pipe', 'pipe'],
    cwd,
    maxBuffer: 64 * 1024 * 1024,
  })
  if (out.error) throw out.error
  return { status: out.status, stdout: out.stdout.toString('utf8'), stderr: out.stderr.toString('utf8') }
}

// The failure: one JSON object on standard error.
function failJson(output: Output): any {
  try {
    return JSON.parse(output.stderr.trim())
  } catch (_e) {
    assert.fail(`one JSON object on stderr, not ${JSON.stringify(output.stderr)}`)
  }
}

function tempFile(name: string, text: string): string {
  const path = join(TMP, name)
  writeFileSync(path, text)
  return path
}

const EXPORT = 'def export [input]\n  pipe input\n    table-from-json api-binding\n    csv csv-options\n'

describe('cli', () => {
  it('the bin is an executable script', () => {
    accessSync(BIN, constants.X_OK)
    // Its interpreter line names node, which POSIX runs and npm reads to
    // write the shim it installs for the command on Windows.
    assert.equal(readFileSync(BIN, 'utf8').split(/\r?\n/)[0], '#!/usr/bin/env node')
    // Run as a script runs it: by its own interpreter line, and on Windows,
    // which runs executables only and reads no interpreter line, by node,
    // as that shim runs it.
    const out =
      'win32' === process.platform
        ? spawnSync(process.execPath, [BIN, 'canon', '-'], { input: 'a b' })
        : spawnSync(BIN, ['canon', '-'], { input: 'a b' })
    if (out.error) throw out.error
    assert.equal(out.status, 0, out.stderr.toString())
    assert.equal(out.stdout.toString(), '(a b)\n')
  })

  it('canon prints the canonical form', () => {
    const file = tempFile('canon.alc', EXPORT)
    const out = dmog(['canon', file])
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, '(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))\n')
    assert.equal(out.stderr, '')
  })

  it('format prints the layout form and reads standard input', () => {
    const out = dmog(['format', '-'], '(def export [input] (pipe input (table-from-json api-binding) (csv csv-options)))')
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, EXPORT)
  })

  it('check is silent on a program that checks', () => {
    const out = dmog(['check', '-'], PROGRAM)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, '')
    assert.equal(out.stderr, '')
  })

  // A library definition over a selection's items, which are typed
  // `Value`: `check` passes it, and `run` checks each one and prints the
  // result.
  it('a definition over selected items checks and runs', () => {
    for (const [name, program, input, want] of [
      [
        'cols.alc',
        'def cols [input] (map public-column (select (path "cols" each-index) input))\ndef export [input] (join "," (map (fn [c] (get :label c)) (cols input)))\n',
        '{"cols":[{"label":"a","source":1},{"label":"b"}]}',
        'a,b',
      ],
      [
        'rows.alc',
        'def export [input]\n  concat-map (partial csv-row csv-options) (select (path "rows" each-index) input)\n',
        '{"rows":[[1,"x"],[2,"y"]]}',
        '"1","x"\r\n"2","y"\r\n',
      ],
    ]) {
      const path = tempFile(name, program)
      let out = dmog(['check', path])
      assert.equal(out.status, 0, `${name}: ${out.stderr}`)
      out = dmog(['run', path, '-'], input)
      assert.equal(out.status, 0, `${name}: ${out.stderr}`)
      assert.equal(out.stdout, want, name)
    }
  })

  it('check reports a reader failure as JSON on stderr with status 2', () => {
    const out = dmog(['check', '-'], 'pipe x\n  f\n  []\n')
    assert.equal(out.status, 2)
    assert.equal(out.stdout, '')
    const fail = failJson(out)
    assert.equal(fail.code, 'DSL_PARSE_ERROR')
    assert.ok(fail.message.startsWith('empty_step: '), JSON.stringify(fail))
    assert.equal(fail.row, 3)
    assert.equal(fail.col, 3)
    assert.equal(fail.output, 'none')
  })

  // `check` reaches the resolver and the checker: an unknown name, a reused
  // stream and a dynamic function each fail with their code, at the
  // position they name, with status 2.
  it('check reports the resolver and checker codes', () => {
    let out = dmog(['check', '-'], EXPORT)
    assert.equal(out.status, 2)
    let fail = failJson(out)
    assert.equal(fail.code, 'DSL_TYPE_ERROR')
    assert.ok(fail.message.startsWith('unknown_name: api-binding'), JSON.stringify(fail))
    assert.deepStrictEqual([fail.row, fail.col], [3, 21])
    out = dmog(['check', '-'], 'def export [input] (concat (json input) (json input))')
    assert.equal(out.status, 2)
    fail = failJson(out)
    assert.equal(fail.code, 'STREAM_REUSED')
    assert.ok(fail.message.startsWith('reused: '), JSON.stringify(fail))
    out = dmog(['check', '-'], 'def a [x] (b x)\ndef b [x] (a x)\ndef export [input] (a input)')
    assert.equal(out.status, 2)
    assert.equal(failJson(out).code, 'STREAMABILITY_UNKNOWN')
  })

  it('explain prints the plan report', () => {
    let out = dmog(['explain', '-'], PROGRAM)
    assert.equal(out.status, 0, out.stderr)
    const text = out.stdout
    assert.ok(text.startsWith('export: api-table → csv\n\nSource reads:          1\n'), text)
    assert.ok(text.includes('Protocol:              JsonEvents/1 → Stream<Selected> → Stream<Value> → TableRows/1 → Text\n'), text)
    assert.ok(
      text.includes('Row capture:           one .response.payload.deep.records[*], capped at max_record_bytes\n'),
      text,
    )
    assert.ok(text.endsWith('A later error can occur after earlier output has been written.\n'), text)
    assert.equal(out.stderr, '')
    out = dmog(['explain', '-'], 'def x 1')
    assert.equal(out.status, 2)
    assert.ok(failJson(out).message.startsWith('no_export: '), out.stderr)
  })

  it('run prints the worked example csv', () => {
    const program = tempFile('export.alc', PROGRAM)
    const input = tempFile('records.json', RECORDS)
    for (const args of [
      ['run', program, input],
      ['run', program, '-'],
      ['run', '-', input],
    ]) {
      const stdin = '-' === args[args.length - 1] ? RECORDS : '-' === args[1] ? PROGRAM : undefined
      const out = dmog(args, stdin)
      assert.equal(out.status, 0, `${args}: ${out.stderr}`)
      assert.equal(out.stdout, EXPECTED_CSV, String(args))
      assert.equal(out.stderr, '', String(args))
    }
  })

  it('run renders a table result as csv or json', () => {
    const program = tempFile('table.alc', PROGRAM.replace('    csv csv-options\n', ''))
    let out = dmog(['run', program, '-'], RECORDS)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, EXPECTED_CSV)
    out = dmog(['run', '--render', 'json', program, '-'], RECORDS)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(
      out.stdout,
      '[{"Identifier":123,"Full name":"Alice","Balance":50.25},{"Identifier":456,"Full name":"Bob","Balance":72}]\n',
    )
    // The echo, and a renderer refused for a program that renders itself.
    const echo = tempFile('echo.alc', 'def export [input] input\n')
    out = dmog(['run', echo, '-'], '[1.50, {"a": null}]')
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, '[1.50,{"a":null}]\n')
    out = dmog(['run', '--render', 'csv', echo, '-'], '[1]')
    assert.equal(out.status, 2)
    assert.ok(failJson(out).message.startsWith('protocol_mismatch: '), out.stderr)
    const text = tempFile('text.alc', PROGRAM)
    out = dmog(['run', '--render', 'json', text, '-'], RECORDS)
    assert.equal(out.status, 2)
    assert.ok(failJson(out).message.startsWith('render_of_text: '), out.stderr)
    // Before the input is read: an input that cannot be read is never
    // reached, so the refusal is the renderer's, not `cannot read`.
    for (const [file, render, prefix] of [
      [echo, 'csv', 'protocol_mismatch: '],
      [text, 'json', 'render_of_text: '],
    ]) {
      out = dmog(['run', '--render', render, file, '/nonexistent/input.json'])
      assert.equal(out.status, 2, prefix)
      assert.ok(failJson(out).message.startsWith(prefix), out.stderr)
    }
  })

  // The statuses follow the code: 1 for an input or protocol failure, 5 for
  // a limit, 2 for the program; the failure is one JSON object on standard
  // error and standard output carries nothing but the answer.
  it('run statuses follow the failure code', () => {
    const program = tempFile('export2.alc', PROGRAM)
    // Metadata after the rows: INPUT_ORDER_VIOLATION, status 1, no output.
    const reordered =
      '{"response":{"payload":{"deep":{"records":[{"id":1,"person":{"name":"a"},"account":{"balance":2}}]}},"metadata":{"fields":[{"title":"Identifier","path":["id"]}]}}}'
    let out = dmog(['run', program, '-'], reordered)
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    let fail = failJson(out)
    assert.equal(fail.code, 'INPUT_ORDER_VIOLATION')
    assert.equal(fail.output, 'none')
    // The library's text refuses it, so the failure names the library's
    // position rather than the row's path.
    assert.ok(String(fail.message).includes('rows must follow their metadata'), JSON.stringify(fail))
    // Invalid JSON after the rows: INPUT_INVALID, status 1; the rows were
    // buffered, not committed, so nothing reached standard output.
    out = dmog(['run', program, '-'], `${RECORDS} x`)
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    fail = failJson(out)
    assert.equal(fail.code, 'INPUT_INVALID')
    assert.equal(fail.output, 'none')
    // A missing cell under the standard options: MISSING_VALUE, status 1.
    const missing =
      '{"response":{"metadata":{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]}]},"payload":{"deep":{"records":[{"id":1}]}}}}'
    out = dmog(['run', program, '-'], missing)
    assert.equal(out.status, 1, out.stderr)
    assert.equal(failJson(out).code, 'MISSING_VALUE')
    // A program that does not check: status 2, nothing read from the input.
    const bad = tempFile('bad.alc', 'def export [input] (nope input)\n')
    out = dmog(['run', bad, '-'], 'not json')
    assert.equal(out.status, 2)
    assert.equal(failJson(out).code, 'DSL_TYPE_ERROR')
  })

  it('a reader error carries the engine code and position', () => {
    const out = dmog(['canon', '-'], 'a\n  b\n c\n')
    assert.equal(out.status, 2)
    const fail = failJson(out)
    assert.equal(fail.code, 'DSL_PARSE_ERROR')
    assert.ok(fail.message.startsWith('bad_dedent: '), JSON.stringify(fail))
    assert.deepStrictEqual([fail.row, fail.col], [3, 2])
  })

  // The reader bounds nesting, so a program nested far past the bound is a
  // `too_deep` failure with status 2 from every command, never a stack
  // overflow.
  it('a program nested beyond the bound is a failure, not an abort', () => {
    const depth = 100_000
    const parens = `${'('.repeat(depth)}x${')'.repeat(depth)}`
    let indented = ''
    for (let level = 0; level < 600; level++) indented += '  '.repeat(level) + 'x\n'
    // Flat to the reader, one level per step to the desugarer.
    const pipe = `pipe x${' f'.repeat(depth)}\n`
    // The failure names the opener, the line or the form that passed the
    // bound: the 256th paren, the line that would open the 256th level, the
    // pipe.
    for (const [command, program, row] of [
      ['canon', parens, 1],
      ['format', parens, 1],
      ['check', parens, 1],
      ['explain', parens, 1],
      ['canon', indented, 257],
      ['check', pipe, 1],
    ] as Array<[string, string, number]>) {
      const out = dmog([command, '-'], program)
      assert.equal(out.status, 2, `${command}: ${out.stderr}`)
      assert.equal(out.stdout, '', command)
      const fail = failJson(out)
      assert.equal(fail.code, 'DSL_PARSE_ERROR', command)
      assert.ok(fail.message.startsWith('too_deep: '), `${command}: ${JSON.stringify(fail)}`)
      assert.equal(fail.row, row, `${command}: ${JSON.stringify(fail)}`)
    }
  })

  it('usage errors and unreadable files exit 2', () => {
    let out = dmog([])
    assert.equal(out.status, 2)
    assert.ok(out.stderr.includes('usage:'), out.stderr)
    assert.equal(out.stdout, '')
    // An unknown command is refused before standard input is read, so what
    // it holds does not matter: unparsable input is not a parse error here.
    out = dmog(['bogus', '-'], '(')
    assert.equal(out.status, 2)
    assert.equal(failJson(out).code, 'INPUT_INVALID')
    assert.equal(out.stdout, '')
    // A wrong argument count, a bad renderer, two standard inputs.
    for (const args of [
      ['run', '-'],
      ['run', '--render', 'xml', 'a.alc', 'b.json'],
      ['run', '--bogus', 'a.alc', 'b.json'],
      ['run', '-', '-'],
      ['explain'],
    ]) {
      out = dmog(args)
      assert.equal(out.status, 2, String(args))
      assert.equal(failJson(out).code, 'INPUT_INVALID', String(args))
      assert.equal(out.stdout, '', String(args))
    }
    out = dmog(['canon', '/nonexistent/program.alc'])
    assert.equal(out.status, 2)
    assert.equal(failJson(out).code, 'INPUT_INVALID')
    assert.equal(out.stdout, '')
    // Malformed UTF-8 cannot be read, from a file or from standard input,
    // as in the Rust binary and the Go command.
    const bad = join(TMP, 'bad.alc')
    writeFileSync(bad, Buffer.concat([Buffer.from('def export [input] '), Buffer.from([0xff, 0x0a])]))
    for (const [file, stdin] of [[bad, undefined], ['-', Buffer.from([0xff])]] as const) {
      out = dmog(['canon', file], stdin)
      assert.equal(out.status, 2, file)
      assert.equal(failJson(out).code, 'INPUT_INVALID', file)
      assert.equal(failJson(out).message, `cannot read ${file}: stream did not contain valid UTF-8`)
      assert.equal(out.stdout, '', file)
    }
  })

  // A program inside the nesting bound checks, runs and explains: the
  // evaluator's stack is its own, where a pipe of 245 steps once overflowed
  // the Rust crate's main thread.
  it('a program inside the bound runs', () => {
    const program = tempFile(
      'pipe245.alc',
      `def export [input]\n  pipe input\n    select (path each-index)\n${'    map (fn [x] x)\n'.repeat(245)}    join ","\n`,
    )
    for (const args of [
      ['check', program],
      ['explain', program],
    ]) {
      const out = dmog(args)
      assert.equal(out.status, 0, `${args}: ${out.stderr}`)
    }
    const out = dmog(['run', program, '-'], '["a","b"]')
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, 'a,b')
  })

  // A function applied to itself is `recursion` with status 2 from `check`
  // (which builds the plan) and from `run`, not a stack overflow.
  it('self-application is a failure, not an abort', () => {
    const omega = tempFile('omega.alc', 'def w [f] (f f)\ndef export [input]\n  let [x (w w)]\n    json input\n')
    let out = dmog(['check', omega])
    assert.equal(out.status, 2, out.stderr)
    const fail = failJson(out)
    assert.equal(fail.code, 'STREAMABILITY_UNKNOWN')
    assert.ok(fail.message.startsWith('recursion: '), JSON.stringify(fail))
    const perItem = tempFile(
      'omega-item.alc',
      'def w [f] (f f)\ndef export [input]\n  pipe input\n    select (path each-index)\n    map (fn [x] (w w))\n    join ","\n',
    )
    out = dmog(['run', perItem, '-'], '[1]')
    assert.equal(out.status, 2, out.stderr)
    assert.equal(failJson(out).code, 'STREAMABILITY_UNKNOWN')
  })

  // `check` builds the plan, so a `fail` on `export`'s own path is reported
  // there, with its code (`INPUT_INVALID`, status 1) and the form's
  // position, although no document was read.
  it('check reports a fail the plan reaches', () => {
    const program = tempFile('fail.alc', 'def export [input] (concat "a" (fail "boom"))\n')
    const out = dmog(['check', program])
    assert.equal(out.status, 1, out.stderr)
    const fail = failJson(out)
    assert.equal(fail.code, 'INPUT_INVALID')
    assert.equal(fail.message, 'boom')
    assert.deepStrictEqual([fail.row, fail.col], [1, 32])
  })

  // A failure inside the standard library names the library's file, row
  // and column in its message, and gives no row or column of the user's
  // file; a program that happens to share a library file's name is still
  // read as its own text.
  it('a library failure names the library file', () => {
    const lib = source('stdlib/table.alc') as string
    const lines = lib.split('\n')
    const index = lines.findIndex((l) => l.includes('fail "Required metadata was not found"'))
    const [row, col] = [index + 1, lines[index].indexOf('fail') + 1]
    const dir = join(TMP, 'lib')
    mkdirSync(join(dir, 'stdlib'), { recursive: true })
    for (const name of ['export.alc', 'stdlib/table.alc']) {
      writeFileSync(join(dir, name), PROGRAM)
      const out = dmog(['run', name, '-'], '{}', dir)
      assert.equal(out.status, 1, `${name}: ${out.stderr}`)
      const fail = failJson(out)
      assert.equal(fail.code, 'INPUT_INVALID', name)
      assert.equal(fail.message, `Required metadata was not found (at stdlib/table.alc:${row}:${col})`, name)
      assert.equal(fail.row, undefined, `${name}: ${JSON.stringify(fail)}`)
    }
  })

  // A standard error that cannot be written loses the report, not the
  // status: the failure's own status comes back.
  it('a closed standard error keeps the status', async () => {
    const bad = tempFile('closed.alc', 'def export [input] (nope input)\n')
    const child = spawn(process.execPath, [BIN, 'check', bad], { stdio: ['ignore', 'ignore', 'pipe'] })
    child.stderr!.destroy()
    const status = await new Promise<number | null>((resolve) => child.on('exit', (code) => resolve(code)))
    assert.equal(status, 2)
  })

  // `--max-output-bytes` bounds what `run` writes, and a text longer than
  // it fails with the limit named, status 5.
  it('run takes an output limit', () => {
    const program = tempFile('limit.alc', PROGRAM)
    let out = dmog(['run', '--max-output-bytes', '20', program, '-'], RECORDS)
    assert.equal(out.status, 5, out.stderr)
    assert.equal(failJson(out).limit.name, 'max_output_bytes')
    assert.ok(Buffer.byteLength(out.stdout) <= 20)
    out = dmog(['run', '--max-output-bytes', '1000', program, '-'], RECORDS)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, EXPECTED_CSV)
    out = dmog(['run', '--max-output-bytes', 'many', program, '-'], RECORDS)
    assert.equal(out.status, 2)
    assert.equal(failJson(out).code, 'INPUT_INVALID')
  })

  // `run --grammar FORMAT` parses INPUT with that format's grammar and runs
  // the program over its rule passes: a transducer over json's writes the
  // document, a number's lexeme kept; a
  // document the grammar refuses is INPUT_INVALID; a format that is not
  // parsed, or is unknown, is refused with status 2, naming the formats
  // that are; a program over JSON events, compiled over parser events, is
  // a protocol mismatch; and the option takes a value.
  it('run takes a grammar whose rule passes the program reads', () => {
    const program = tempFile('transducer.alc', TRANSDUCER)
    const doc = '{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}'
    let out = dmog(['run', '--grammar', 'json', program, '-'], doc)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, `${doc}\n`)
    out = dmog(['run', '--grammar', 'json', program, '-'], '[1,')
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.equal(failJson(out).code, 'INPUT_INVALID')
    for (const name of ['expr', 'nope']) {
      out = dmog(['run', '--grammar', name, program, '-'], doc)
      assert.equal(out.status, 2, out.stderr)
      const fail = failJson(out)
      assert.equal(fail.code, 'INPUT_INVALID')
      assert.equal(
        fail.message,
        '--grammar takes a format whose documents are parsed (abnf, c, css, csv, ebnf, feed, gbnf, ini, json, ' +
          `json5, jsonc, jsonic, jsonl, markdown, pgn, proto, semver, toml, xml, yaml or zon), not ${JSON.stringify(name)}`,
      )
    }
    const events = tempFile('events.alc', 'def export [input] (json input)\n')
    out = dmog(['run', '--grammar', 'json', events, '-'], doc)
    assert.notEqual(out.status, 0)
    const fail = failJson(out)
    assert.equal(fail.code, 'DSL_TYPE_ERROR')
    assert.ok(fail.message.startsWith('protocol_mismatch'), fail.message)
    out = dmog(['run', program, '-', '--grammar'], doc)
    assert.equal(out.status, 2)
  })
})

// The json transducer: the library's `transduce-tree` over the json
// grammar's rules, as its package's part spells it.
const TRANSDUCER =
  'def export [input]\n' +
  '  transduce-tree\n' +
  '    record\n' +
  '      entry :map (record (entry :container :object))\n' +
  '      entry :list (record (entry :container :array))\n' +
  '      entry :pair (record (entry :key 0))\n' +
  '      entry :val (record (entry :scalar 0))\n' +
  '    input\n'

// `translate` and `formats`, as the Rust binary has them: a document read
// with one format's grammar and written in another, the flags, and the
// failures with their statuses. ./translate.test.ts writes every document
// of its corpus into every format.
describe('cli translate', () => {
  const PEOPLE = '{"people":[{"name":"Ann","age":31.50},{"name":"Bo","age":7}],"n":null}'
  const IDS = [
    'abnf',
    'c',
    'css',
    'csv',
    'ebnf',
    'expr',
    'feed',
    'gbnf',
    'ini',
    'json',
    'json5',
    'jsonc',
    'jsonic',
    'jsonl',
    'markdown',
    'pgn',
    'proto',
    'semver',
    'toml',
    'xml',
    'yaml',
    'zon',
  ]
  const TABLE =
    'def export [input]\n' +
    '  table-from-json (record (entry :columns :infer) (entry :rows (path "people" each-index))) input\n'
  const TEXT = 'def export [input] (join "," (map (fn [p] (get :name p)) (select (path "people" each-index) input)))\n'

  it('formats prints the formats translate reads and writes, as JSON', () => {
    const out = dmog(['formats'])
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stderr, '')
    assert.ok(out.stdout.endsWith('}]\n'), out.stdout)
    const formats = JSON.parse(out.stdout)
    assert.deepStrictEqual(
      formats.map((f: any) => f.id),
      IDS,
    )
    for (const f of formats) {
      assert.deepStrictEqual(Object.keys(f), [
        'id',
        'reads',
        'writes',
        'root',
        'schema',
        'whole',
        'lift',
        'embed',
        'render',
        'transduce',
        'loss',
      ])
      assert.ok(0 < f.loss.length, `${f.id} declares what it does not keep`)
    }
    const byId = (id: string) => formats.find((f: any) => id === f.id)
    assert.deepStrictEqual(byId('json'), {
      id: 'json',
      reads: ['tree'],
      writes: 'tree',
      root: 'any',
      schema: null,
      whole: null,
      lift: null,
      embed: null,
      render: 'json',
      transduce: 'json-transduce',
      loss: ['JSON has no spelling for Infinity or NaN, so a number that is not finite is written as null.'],
    })
    // The formats read through a transducer part are those whose packages
    // ship one, each under the entry its manifest names, and no other. CSS,
    // INI, TOML and feed ship theirs from their next release, so each is
    // held to the package this build has, whichever side of that release:
    // the registry's copy, or the sibling checkout the polyglot gate links.
    const shipped = (id: string, parts: any): string[][] =>
      parts?.transduce ? [[id, parts.transduce.entry]] : []
    assert.deepStrictEqual(
      formats.filter((f: any) => null !== f.transduce).map((f: any) => [f.id, f.transduce]),
      [
        ['abnf', 'abnf-transduce'],
        ...shipped('css', cssParts()),
        ['csv', 'csv-transduce'],
        ['ebnf', 'ebnf-transduce'],
        ...shipped('feed', feedParts()),
        ['gbnf', 'gbnf-transduce'],
        ...shipped('ini', iniParts()),
        ['json', 'json-transduce'],
        ['json5', 'json5-transduce'],
        ['jsonc', 'jsonc-transduce'],
        ['jsonic', 'jsonic-transduce'],
        ['jsonl', 'jsonl-transduce'],
        ['markdown', 'markdown-transduce'],
        ['pgn', 'pgn-transduce'],
        ['proto', 'proto-transduce'],
        ...shipped('toml', tomlParts()),
        ['xml', 'xml-transduce'],
        ['yaml', 'yaml-transduce'],
        ['zon', 'zon-transduce'],
      ],
    )
    const markdown = byId('markdown')
    assert.deepStrictEqual(
      [markdown.reads, markdown.writes, markdown.root, markdown.schema, markdown.lift, markdown.embed, markdown.render],
      [['records', 'tree'], 'records', 'array', 'markdown-ast', 'markdown-lift', null, 'markdown-render'],
    )
    const xml = byId('xml')
    assert.deepStrictEqual([xml.schema, xml.embed, xml.render], ['xml-element', 'xml-embed', 'xml-render'])
    // The formats that write a schema's tree with no embedding into it are
    // schema-only: the composition refuses any other format's tree for them.
    assert.deepStrictEqual(
      formats.filter((f: any) => null !== f.schema && 'tree' === f.writes && null === f.embed).map((f: any) => f.id),
      ['abnf', 'c', 'css', 'ebnf', 'gbnf', 'pgn', 'proto'],
    )
    // The grammar notations share a schema, a grammar's rules in source
    // order, so each writes the others' documents.
    assert.deepStrictEqual(
      ['abnf', 'ebnf', 'gbnf', 'c'].map((id) => byId(id).schema),
      ['grammar-rules', 'grammar-rules', 'grammar-rules', 'c'],
    )
    assert.deepStrictEqual(
      [byId('pgn').schema, byId('semver').embed, byId('feed').embed, byId('expr').embed],
      ['pgn-database', 'semver-embed', 'feed-embed', 'expr-embed'],
    )
    // Why a format's documents are read whole, where its manifest says:
    // every format transduce does not stream, each in the sentence its
    // manifest gives; none for a format read through its transducer part,
    // PGN's among them, which streams a game at a time, proto's, a
    // declaration at a time, Markdown's, a block at a time, and the grammar
    // notations', a rule at a time.
    for (const [id, parts] of [
      ['c', cParts()],
      ['expr', exprParts()],
      ['semver', semverParts()],
    ] as const) {
      const whole = JSON.parse(String(parts?.manifest)).translate.whole
      assert.ok('string' === typeof whole && '' !== whole, id)
      assert.strictEqual(byId(id).whole, whole, id)
    }
    assert.strictEqual(byId('markdown').whole, null)
    assert.strictEqual(byId('pgn').whole, null)
    assert.strictEqual(byId('proto').whole, null)
    for (const [id, parts] of [
      ['abnf', abnfParts()],
      ['ebnf', ebnfParts()],
      ['gbnf', gbnfParts()],
    ] as const) {
      assert.strictEqual(byId(id).whole, null, id)
      assert.strictEqual(byId(id).transduce, parts?.transduce?.entry, id)
    }
    // CSS, INI, TOML and feed give the sentence up with the part they ship
    // from their next release: each is held to the package this build has,
    // and has one of the two, never both or neither.
    for (const [id, parts] of [
      ['css', cssParts()],
      ['feed', feedParts()],
      ['ini', iniParts()],
      ['toml', tomlParts()],
    ] as const) {
      const whole = JSON.parse(String(parts?.manifest)).translate.whole
      assert.strictEqual(byId(id).whole, 'string' === typeof whole ? whole : null, id)
      assert.strictEqual(null === byId(id).transduce, 'string' === typeof whole, id)
    }
    // `formats` takes no arguments.
    const extra = dmog(['formats', 'json'])
    assert.equal(extra.status, 2)
    assert.equal(failJson(extra).code, 'INPUT_INVALID')
    assert.ok(failJson(extra).message.startsWith('usage: '), extra.stderr)
    assert.equal(extra.stdout, '')
  })

  it('translate writes a document in another format', () => {
    const input = tempFile('people.json', PEOPLE)
    let out = dmog(['translate', '--from', 'json', '--to', 'yaml', input])
    assert.equal(out.status, 0, out.stderr)
    // A number keeps the lexeme the document spelled it with.
    assert.equal(
      out.stdout,
      '"people":\n  - "name": "Ann"\n    "age": 31.50\n  - "name": "Bo"\n    "age": 7\n"n": null\n',
    )
    // What a format does not keep is its loss list, which `formats` prints:
    // a translation writes the document and nothing else.
    assert.equal(out.stderr, '')
    for (const [args, stdin, want] of [
      [
        ['--from', 'json', '--to', 'toml', '-'],
        PEOPLE,
        '"people" = [ { "name" = "Ann", "age" = 31.50 }, { "name" = "Bo", "age" = 7 } ]\n',
      ],
      [['--from', 'json', '--to', 'jsonl', '-'], PEOPLE, `${PEOPLE}\n`],
      [['--from', 'json', '--to', 'xml', '-'], '42', '<document type="number">42</document>\n'],
      [['--from', 'csv', '--to', 'json', '-'], 'a,b\n1,x\n', '[{"a":"1","b":"x"}]\n'],
      [['--from', 'csv', '--to', 'markdown', '-'], 'a,b\n1,x\n', '| a | b |\n| --- | --- |\n| 1 | x |\n'],
      // A YAML stream of several documents, which YAML's transducer part
      // cannot follow, is read with --whole, as the grammar's own value.
      [['--whole', '--from', 'yaml', '--to', 'json', '-'], 'a: 1\n---\nb: 2\n', '[{"a":1},{"b":2}]\n'],
      // A path selects in that array.
      [['--whole', '--from', 'yaml', '--to', 'json', '--path', '[1,"b"]', '-'], 'a: 1\n---\nb: 2\n', '2\n'],
      // A byte order mark is the document's, and its grammar's to read.
      [['--from', 'csv', '--to', 'json', '-'], '\ufeffa\n1\n', '[{"\ufeffa":"1"}]\n'],
    ] as Array<[string[], string, string]>) {
      out = dmog(['translate', ...args], stdin)
      assert.equal(out.status, 0, `${args}: ${out.stderr}`)
      assert.equal(out.stdout, want, String(args))
      assert.equal(out.stderr, '', String(args))
    }
    // Without --whole the output streams, so a document the part cannot
    // follow fails with its code and names the route that reads it
    // (admin ADR-33): a second root, a repeated member.
    for (const [args, stdin, code, status] of [
      [['--from', 'yaml', '--to', 'json', '-'], 'a: 1\n---\nb: 2\n', 'STREAMABILITY_UNKNOWN', 2],
      [['--from', 'json', '--to', 'yaml', '-'], '{"a":1,"a":2}', 'DUPLICATE_MEMBER', 1],
      // Under a path as at the root.
      [['--from', 'yaml', '--to', 'json', '--path', '["b"]', '-'], 'a: 1\n---\nb: 2\n', 'STREAMABILITY_UNKNOWN', 2],
      [['--from', 'json', '--to', 'yaml', '--path', '["a"]', '-'], '{"a":1,"a":2}', 'DUPLICATE_MEMBER', 1],
    ] as Array<[string[], string, string, number]>) {
      out = dmog(['translate', ...args], stdin)
      assert.equal(out.status, status, `${args}: ${out.stderr}`)
      assert.equal(failJson(out).code, code, out.stderr)
      assert.ok(failJson(out).message.endsWith('; --whole reads the document whole'), out.stderr)
    }
  })

  // `--key` names the member a root that is not an object is written under,
  // for a format whose document is one; `--path` selects the value to write.
  it('translate takes a key and a path', () => {
    const input = tempFile('people-path.json', PEOPLE)
    const number = tempFile('number.json', '42')
    for (const [args, want] of [
      [['--to', 'toml', number], '"items" = 42\n'],
      [['--to', 'toml', '--key', 'value', number], '"value" = 42\n'],
      [['--to', 'ini', '--key', 'value', number], '"value" = 42\n'],
      // A format whose document may be any value takes no key.
      [['--to', 'yaml', '--key', 'value', number], '42\n'],
      [['--to', 'toml', '--key', 'age', '--path', '["people",1,"age"]', input], '"age" = 7\n'],
      // A selected value streams through the part, as the root does, so a
      // number keeps the lexeme the document spelled it with.
      [['--to', 'csv', '--path', '["people"]', input], '"name","age"\r\n"Ann","31.50"\r\n"Bo","7"\r\n'],
      // Read whole, the path selects in the grammar's value.
      [['--whole', '--to', 'csv', '--path', '["people"]', input], '"name","age"\r\n"Ann","31.5"\r\n"Bo","7"\r\n'],
      [['--to', 'json', '--path', '[]', input], `${PEOPLE}\n`],
    ] as Array<[string[], string]>) {
      const out = dmog(['translate', '--from', 'json', ...args])
      assert.equal(out.status, 0, `${args}: ${out.stderr}`)
      assert.equal(out.stdout, want, String(args))
    }
    // A path that names nothing is the document's failure, status 1.
    let out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--path', '["people",5]', input])
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'INPUT_INVALID',
      message: 'the path ["people",5] names nothing in the document',
      output: 'none',
    })
    // A path that is not an array of keys and indexes is the command line's,
    // status 2, before the input is read.
    for (const path of ['people', '[0.5]', '[-1]', '[true]', '{"a":1}']) {
      out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--path', path, '/nonexistent/input.json'])
      assert.equal(out.status, 2, `${path}: ${out.stderr}`)
      assert.deepStrictEqual(failJson(out), {
        code: 'INPUT_INVALID',
        message: `--path takes a JSON array of keys and indexes, such as ["people",0], not ${path}`,
        output: 'none',
      })
    }
  })

  // `--with` runs a program over the input first; its export's events or
  // table are what the format writes.
  it('translate takes a program', () => {
    const input = tempFile('people-with.json', PEOPLE)
    const table = tempFile('table.alc', TABLE)
    for (const [to, want] of [
      ['csv', '"name","age"\r\n"Ann","31.50"\r\n"Bo","7"\r\n'],
      ['markdown', '| name | age |\n| --- | --- |\n| Ann | 31.50 |\n| Bo | 7 |\n'],
      ['json', '[{"name":"Ann","age":31.50},{"name":"Bo","age":7}]\n'],
    ]) {
      const out = dmog(['translate', '--from', 'json', '--to', to, '--with', table, input])
      assert.equal(out.status, 0, `${to}: ${out.stderr}`)
      assert.equal(out.stdout, want, to)
    }
    // The program from standard input.
    let out = dmog(['translate', '--from', 'json', '--to', 'csv', '--with', '-', input], TABLE)
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, '"name","age"\r\n"Ann","31.50"\r\n"Bo","7"\r\n')
    // A program that writes its own text has nothing for a render to write:
    // refused before the input is read.
    const text = tempFile('text-export.alc', TEXT)
    out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--with', text, '/nonexistent/input.json'])
    assert.equal(out.status, 2, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'DSL_TYPE_ERROR',
      message:
        `bad_output: ${text}'s export writes its own text, and translate takes a program whose export ` +
        "answers JSON events or a table, which a format's render then writes",
      output: 'none',
    })
    // A program that does not check, at its position.
    const broken = tempFile('broken-export.alc', 'def export [input] (nope input)\n')
    out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--with', broken, input])
    assert.equal(out.status, 2, out.stderr)
    const fail = failJson(out)
    assert.equal(fail.code, 'DSL_TYPE_ERROR')
    assert.ok(fail.message.startsWith('unknown_name: nope'), JSON.stringify(fail))
    assert.deepStrictEqual([fail.row, fail.col], [1, 21])
  })

  // What a package's own parse checks before its rules run, the command
  // checks too, status 1, writing nothing: a JSON5 document holding no
  // value is refused with json5's own codes, not the engine's unexpected,
  // from a file and from standard input, as the Rust and Go commands refuse
  // it (json5's TypeScript error carries no position, theirs 1:1); a .proto
  // file nesting past proto's cap is refused before the engine runs (its
  // parse's preflight). A grammar notation's document is read as its rules,
  // which no compiler has seen, so one its compiler would refuse is read as
  // it is.
  it('translate checks what a package checks before its parse', () => {
    for (const [input, code] of [
      ['', 'json5_empty'],
      ['// c\n', 'json5_no_value'],
    ]) {
      const file = tempFile(`${code}.json5`, input)
      for (const [path, stdin] of [
        [file, undefined],
        ['-', input],
      ] as const) {
        const out = dmog(['translate', '--from', 'json5', '--to', 'json', path], stdin)
        assert.equal(out.status, 1, `${path}: ${out.stderr}`)
        assert.equal(out.stdout, '', path)
        assert.deepStrictEqual(
          failJson(out),
          {
            code: 'INPUT_INVALID',
            message: `${code}: JSON5 input must contain a value`,
            output: 'none',
          },
          path,
        )
      }
    }
    let out = dmog(['translate', '--from', 'json5', '--to', 'json', '-'], '// c\n{a:1}\n')
    assert.equal(out.status, 0, out.stderr)
    assert.equal(out.stdout, '{"a":1}\n')
    const deep = (n: number): string =>
      'syntax = "proto3";\n' + 'message M {'.repeat(n) + '}'.repeat(n) + '\n'
    out = dmog(['translate', '--from', 'proto', '--to', 'json', '-'], deep(101))
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'INPUT_INVALID',
      message: 'proto: document nests 101 levels deep, past the 100 this parser accepts',
      output: 'none',
    })
    // A grammar notation's document is read as its rules in source order: a
    // rule referencing one the grammar does not define is read as it is,
    // and so is a left recursion through a cycle of twenty rules, each with
    // two recursive alternates, which tabnas-bnf's elimination refuses past
    // 512 alternatives.
    out = dmog(['translate', '--from', 'abnf', '--to', 'json', '-'], 'a = "b" c\n')
    assert.equal(out.status, 0, out.stderr)
    assert.equal(
      out.stdout,
      '{"type":"grammar","rules":[{"type":"rule","name":"a","alternatives":' +
        '[[{"type":"literal","text":"b","caseSensitive":false},{"type":"reference","name":"c"}]]}]}\n',
    )
    const k = 20
    let cycle = 'root ::= r0\n'
    for (let i = 0; i < k; i++) {
      const next = (i + 1) % k
      cycle += `r${i} ::= r${next} "a${i}" | r${next} "b${i}" | "x${i}"\n`
    }
    out = dmog(['translate', '--from', 'gbnf', '--to', 'json', '-'], cycle)
    assert.equal(out.status, 0, out.stderr)
    const tree = JSON.parse(out.stdout)
    assert.equal(tree.start, 'root')
    assert.equal(tree.rules.length, k + 1)
  })

  // What a target declares it cannot carry it refuses before writing
  // anything, status 1: a schema-only target refuses another format's tree
  // when the route is composed, before the input is read, a grammar
  // notation's render refuses a grammar it has no form for, naming the rule
  // and what it met, and Semantic Versioning's embedding refuses a tree that
  // is not a version. Each takes its own format's documents, and a program
  // that makes its tree; a grammar notation takes the others' too.

  it('translate refuses what a target declares it cannot carry', () => {
    let out = dmog(['translate', '--from', 'json', '--to', 'css', '/nonexistent/input.json'])
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'TARGET_VALUE_UNREPRESENTABLE',
      message:
        'schema_only: css writes a css-ast tree, the tree its own documents read as, and this document is not ' +
        'one; a program that makes one can be composed with the render',
      output: 'none',
    })
    out = dmog(['translate', '--from', 'json', '--to', 'semver', '-'], '{"major":1,"minor":2}')
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'TARGET_VALUE_UNREPRESENTABLE',
      message:
        'the document is not a version: it has no patch; a version is an object whose major, minor and patch ' +
        'are whole numbers written in digits, with an optional prerelease and build, each a list of identifiers ' +
        'or one string of them',
      row: 48,
      col: 3,
      file: 'tabnas-semver/alchemy/embed.alc',
      output: 'none',
    })
    for (const [to, schema] of [
      ['abnf', 'grammar-rules'],
      ['c', 'c'],
    ]) {
      out = dmog(['translate', '--from', 'json', '--to', to, '-'], '{}')
      assert.equal(out.status, 1, out.stderr)
      assert.equal(out.stdout, '')
      assert.deepStrictEqual(failJson(out), {
        code: 'TARGET_VALUE_UNREPRESENTABLE',
        message:
          `schema_only: ${to} writes a ${schema} tree, the tree its own documents read as, and this document is ` +
          'not one; a program that makes one can be composed with the render',
        output: 'none',
      })
    }
    out = dmog(['translate', '--from', 'abnf', '--to', 'ebnf', '-'], 'greet = "hi"\n')
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    assert.deepStrictEqual(failJson(out), {
      code: 'TARGET_VALUE_UNREPRESENTABLE',
      message:
        'the grammar cannot be written as EBNF: the rule "greet" holds the case-insensitive literal "hi", ' +
        'which holds a letter beside another character, and no one EBNF terminal spells',
      row: 83,
      col: 3,
      file: 'tabnas-ebnf/alchemy/render.alc',
      output: 'none',
    })
    const tree = '{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":' +
      '[{"type":"declaration","property":"color","value":"red"}]}]}'
    const echo = tempFile('echo-css.alc', 'def export [input] input\n')
    for (const [args, stdin, want] of [
      [['--from', 'json', '--to', 'semver', '-'], '{"major":1,"minor":2,"patch":3,"prerelease":"rc.1"}', '1.2.3-rc.1'],
      [['--from', 'semver', '--to', 'json', '-'], '1.2.3-rc.1', '{"major":1,"minor":2,"patch":3,"prerelease":["rc",1],"build":[]}\n'],
      [['--from', 'css', '--to', 'css', '-'], 'a{color:red}', 'a {\n  color: red;\n}\n'],
      [['--from', 'json', '--to', 'css', '--with', echo, '-'], tree, 'a {\n  color: red;\n}\n'],
      [['--from', 'pgn', '--to', 'pgn', '-'], '1. e4 e5 1-0', '1. e4 e5 1-0\n'],
      [['--from', 'json', '--to', 'expr', '-'], '{"a":[1,-2]}', '{"a":[1,-2]}\n'],
      [
        ['--from', 'proto', '--to', 'proto', '-'],
        'syntax = "proto3";\nmessage M { int32 a = 1; }\n',
        'syntax = "proto3";\nmessage M {\n  int32 a = 1;\n}\n',
      ],
      // White space after the last token is not in the tree (C's loss
      // list), so the final line feed is not written.
      [['--from', 'c', '--to', 'c', '-'], 'int main(void) { return 0; }\n', 'int main(void) { return 0; }'],
      [['--from', 'ebnf', '--to', 'abnf', '-'], 'top ::= "a" b\nb ::= "c"\n', 'top = %s"a" b\nb = %s"c"\n'],
      [['--from', 'ebnf', '--to', 'gbnf', '-'], 'top ::= "a" b\nb ::= "c"\n', 'top ::= "a" b\nb ::= "c"\nroot ::= top\n'],
      [['--from', 'abnf', '--to', 'ebnf', '-'], 'top = "a"\n', 'top ::= [aA]\n'],
    ] as Array<[string[], string, string]>) {
      out = dmog(['translate', ...args], stdin)
      assert.equal(out.status, 0, `${args}: ${out.stderr}`)
      assert.equal(out.stdout, want, String(args))
      assert.equal(out.stderr, '', String(args))
    }
  })

  it('translate failures exit with their statuses', () => {
    const input = tempFile('people-fail.json', PEOPLE)
    // Usage errors, status 2, nothing read: no --from, no input, two inputs,
    // a flag without its value, a flag translate does not take.
    for (const args of [
      ['--to', 'yaml', input],
      ['--from', 'json', '--to', 'yaml'],
      ['--from', 'json', '--to', 'yaml', input, input],
      ['--from', 'json', '--to'],
      ['--from', 'json', '--to', 'yaml', '--render', 'json', input],
    ]) {
      const out = dmog(['translate', ...args])
      assert.equal(out.status, 2, String(args))
      assert.equal(out.stdout, '', String(args))
      assert.ok(failJson(out).message.startsWith('usage: '), out.stderr)
    }
    // A format no package names, two standard inputs, an output limit that
    // is not a number: status 2, with what was wrong.
    for (const [args, message] of [
      [['--from', 'json', '--to', 'docx', input], `--to takes ${IDS.slice(0, -1).join(', ')} or zon, not "docx"`],
      [['--from', 'JSON', '--to', 'yaml', input], `--from takes ${IDS.slice(0, -1).join(', ')} or zon, not "JSON"`],
      [
        ['--from', 'json', '--to', 'yaml', '--with', '-', '-'],
        'only one of PROGRAM and INPUT may be - (standard input)',
      ],
      [
        ['--from', 'json', '--to', 'yaml', '--max-output-bytes', 'many', input],
        '--max-output-bytes takes a number of bytes, not "many"',
      ],
    ] as Array<[string[], string]>) {
      const out = dmog(['translate', ...args])
      assert.equal(out.status, 2, String(args))
      assert.equal(out.stdout, '', String(args))
      assert.deepStrictEqual(failJson(out), { code: 'INPUT_INVALID', message, output: 'none' }, String(args))
    }
    // An input that cannot be read: status 2.
    let out = dmog(['translate', '--from', 'json', '--to', 'yaml', '/nonexistent/input.json'])
    assert.equal(out.status, 2, out.stderr)
    assert.ok(failJson(out).message.startsWith('cannot read /nonexistent/input.json: '), out.stderr)
    // A document its grammar refuses: status 1, at the engine's position.
    out = dmog(['translate', '--from', 'json', '--to', 'yaml', '-'], '{"a":')
    assert.equal(out.status, 1, out.stderr)
    assert.equal(out.stdout, '')
    let fail = failJson(out)
    assert.equal(fail.code, 'INPUT_INVALID')
    assert.deepStrictEqual([fail.row, fail.col], [1, 6])
    out = dmog(['translate', '--from', 'toml', '--to', 'json', input])
    assert.equal(out.status, 1, out.stderr)
    assert.deepStrictEqual([failJson(out).code, failJson(out).row, failJson(out).col], ['INPUT_INVALID', 1, 1])
    // An output past `--max-output-bytes`: status 5, the limit named, and
    // nothing written, since a translation is written once it has succeeded.
    out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--max-output-bytes', '10', input])
    assert.equal(out.status, 5, out.stderr)
    assert.equal(out.stdout, '')
    fail = failJson(out)
    assert.equal(fail.code, 'RESOURCE_LIMIT_EXCEEDED')
    assert.deepStrictEqual(fail.limit, { name: 'max_output_bytes', value: 10 })
    out = dmog(['translate', '--from', 'json', '--to', 'yaml', '--max-output-bytes', '1000', input])
    assert.equal(out.status, 0, out.stderr)
  })
})

// `run` and `translate` read INPUT through a reader, as the Rust binary
// does: a file opened, or standard input, read once. Where the source's
// grammar declares `lex.window` the engine reads it a window at a time, so
// the output streams before the input ends; JSON's does from the release
// of @tabnas/json that declares it, and before that release the engine
// reads the reader to its end first. Each test is held to the package this
// build has, whichever side of that release.
describe('cli reads INPUT through a reader', () => {
  const windowed = true === (makeJson() as any).options?.lex?.window
  const records = Array.from({ length: 20000 }, (_, i) => `{"id":${i},"name":"n${i}"}`).join(',')
  const echo = tempFile('stream-echo.alc', 'def export [input] input\n')
  const commands = [
    ['run', echo],
    ['translate', '--from', 'json', '--to', 'json'],
  ]

  // Bytes that are not UTF-8 near the end of a long document fail the run
  // as a file that cannot be read, from a file and from standard input
  // alike: after the output of the document before them, which the failure
  // calls partial, where the grammar reads a window at a time, and before
  // any output where the engine reads the whole document first.
  it('a long document with bytes that are not UTF-8 near its end cannot be read', () => {
    const good = tempFile('stream-good.json', `[${records}]`)
    const bytes = Buffer.concat([Buffer.from(`[${records},"`), Buffer.from([0xff]), Buffer.from('"]')])
    const bad = join(TMP, 'stream-bad.json')
    writeFileSync(bad, bytes)
    for (const command of commands) {
      const whole = dmog([...command, good])
      assert.equal(whole.status, 0, whole.stderr)
      for (const [input, stdin] of [
        [bad, undefined],
        ['-', bytes],
      ] as const) {
        const at = `${command[0]} ${input}`
        const out = dmog([...command, input], stdin)
        assert.equal(out.status, 2, `${at}: ${out.stderr}`)
        const fail = failJson(out)
        assert.equal(fail.code, 'INPUT_INVALID', at)
        assert.equal(fail.message, `cannot read ${input}: stream did not contain valid UTF-8`, at)
        if (windowed) {
          assert.equal(fail.output, 'partial', at)
          assert.ok(out.stdout.length >= 32 * 1024, `${at}: ${out.stdout.length} bytes`)
          assert.ok(whole.stdout.startsWith(out.stdout), at)
        } else {
          assert.equal(fail.output, 'none', at)
          assert.equal(out.stdout, '', at)
        }
      }
    }
  })

  // Run `args` over standard input held open after `first`, which it must
  // write from within 60 s; then `rest` follows, and the run must end with
  // status 0. Resolves with everything it wrote.
  async function writtenBeforeTheEnd(args: string[], first: Buffer, rest: Buffer): Promise<string> {
    const child = spawn(process.execPath, [BIN, ...args, '-'], { stdio: ['pipe', 'pipe', 'pipe'] })
    const chunks: Buffer[] = []
    let errors = ''
    child.stderr.on('data', (data: Buffer) => (errors += data.toString('utf8')))
    const exited = new Promise<number | null>((resolve) => child.once('exit', (code) => resolve(code)))
    const started = new Promise<void>((resolve, reject) => {
      child.stdout.on('data', (data: Buffer) => {
        chunks.push(data)
        resolve()
      })
      exited.then((code) => reject(new Error(`${args.join(' ')} exited ${code} before it wrote: ${errors}`)))
    })
    let timer: NodeJS.Timeout | undefined
    const late = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${args.join(' ')} wrote nothing in 60 s of half a document`)), 60000)
    })
    child.stdin.write(first)
    try {
      await Promise.race([started, late])
    } catch (err) {
      child.kill()
      throw err
    } finally {
      clearTimeout(timer)
    }
    assert.ok(chunks.length > 0, args.join(' '))
    const ended = new Promise<void>((resolve) => child.stdout.once('end', () => resolve()))
    child.stdin.end(rest)
    assert.equal(await exited, 0, `${args.join(' ')}: ${errors}`)
    await ended
    return Buffer.concat(chunks).toString('utf8')
  }

  // A long document's output starts before its input ends: standard input
  // is held open after half the document, and the command has written by
  // then. The other half follows, and the run ends as it would have.
  it('a long document is written before its input ends', async (t) => {
    if (!windowed) {
      t.skip('@tabnas/json declares no lex.window in this build')
      return
    }
    for (const command of commands) {
      await writtenBeforeTheEnd(command, Buffer.from(`[${records}`), Buffer.from(`,${records}]`))
    }
  })

  // So is a document whose root is an object, all of it in one member.
  // render's JSON writer makes each member of a root object one record, and
  // the command's writer holds no more than render's budget of a record
  // still open (src/translate.ts's `renderers`), so the member is written as
  // it comes, and the run ends as one over the whole document does.
  it('a document whose root is an object with one long member is written before its input ends', async (t) => {
    if (!windowed) {
      t.skip('@tabnas/json declares no lex.window in this build')
      return
    }
    const rows = Array.from({ length: 6000 }, (_, i) => `{"id":${i},"name":"n${i}"}`).join(',')
    const whole = tempFile('stream-object.json', `{"rows":[${rows},${rows}]}`)
    for (const command of commands) {
      const written = await writtenBeforeTheEnd(command, Buffer.from(`{"rows":[${rows}`), Buffer.from(`,${rows}]}`))
      const once = dmog([...command, whole])
      assert.equal(once.status, 0, once.stderr)
      assert.equal(written, once.stdout, command[0])
    }
  })

  // So is a JSON5 document, whose parser reads only as far as its leading
  // comment to check it holds a value, a ZON document and a YAML document:
  // their packages' `make()` declares `lex.window` too.
  it('a JSON5, ZON or YAML document is written before its input ends', async () => {
    const each = (item: (i: number) => string, separator: string): string =>
      Array.from({ length: 20000 }, (_, i) => item(i)).join(separator)
    const json5 = each((i) => `{id:${i},name:'n${i}'}`, ',')
    const zon = each((i) => `.{ .id = ${i}, .name = "n${i}" }`, ', ')
    const yaml = each((i) => `- id: ${i}\n  name: n${i}\n`, '')
    for (const [from, first, rest] of [
      ['json5', `// records\n[${json5}`, `,${json5},]`],
      ['zon', `.{ ${zon}`, `, ${zon} }`],
      ['yaml', `# records\n${yaml}`, yaml],
    ]) {
      await writtenBeforeTheEnd(['translate', '--from', from, '--to', 'json'], Buffer.from(first), Buffer.from(rest))
    }
  })

  // So does the value a path names below the root: a sub-stream of the
  // document's events, written as they come, and all of the value.
  it("a path's value is written before its input ends", async (t) => {
    if (!windowed) {
      t.skip('@tabnas/json declares no lex.window in this build')
      return
    }
    const lines = Array.from({ length: 20000 }, (_, i) => `{"id":${i},"name":"n${i}"}`)
    const written = await writtenBeforeTheEnd(
      ['translate', '--from', 'json', '--to', 'jsonl', '--path', '["rows"]'],
      Buffer.from(`{"meta":{"n":2},"rows":[${records}`),
      Buffer.from(`,${records}],"after":true}`),
    )
    assert.equal(written, `${[...lines, ...lines].join('\n')}\n`)
  })
})
