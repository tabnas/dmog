/* Copyright (c) 2026 tabnas, MIT License */

// The standard library's text on transduce's routers and render's
// renderers (rs/tests/stdlib_test.rs): the one implementation of
// `table-from-json`, `csv`, `transduce-tree` and `transduce-records` (the
// ruling of 2026-10-24: nothing native stands in for the text), over every
// transduce fixture a grammar these tests can read, the generated
// documents transduce's tests and benches share, tabnas-csv's corpus and
// the documents below. Where an oracle exists the text is held to it: the
// spec's worked-example program writes the bytes transduce's table
// transducer and render's CSV renderer write for the same binding, or
// fails with their code; a part whose spec is the grammar's writes the
// walk of the grammar's own value, as transduce.tsv's rule says; and the
// library's `csv` refuses what the renderer refuses.

import { describe, it } from 'node:test'
import assert from 'node:assert'
import { readFileSync, readdirSync } from 'node:fs'
import { extname, join } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { make as makeCsv } from '@tabnas/csv'
import { make as makeJson } from '@tabnas/json'
import { make as makeJsonl } from '@tabnas/jsonl'
import { Yaml } from '@tabnas/yaml'
import { BytesWriter, renderers } from '@tabnas/render'
import { Guarded, ParserEventSource, TableFromJson } from '@tabnas/transduce'

import { Program } from '@tabnas/alchemy'
import { AbortFlag, Code, DatumBuilder, Ev, Limits, Metrics, Schema, Selector, fromTabnas, replay } from '@tabnas/alchemy/shared'
import { loadSpecDir } from '@tabnas/support'

import { EXPECTED_CSV, PROGRAM, RECORDS } from './common'
import { Collect, compile, compileWith, events, jsonInto, plainEvents, replayed, vendored } from './host'
import { FIXTURES, METADATA, record, recordsCsv, recordsJson, recordsJsonl, recordsYaml } from './support'

type Make = () => any

// The grammar for a fixture, by extension, among those these tests take;
// undefined skips the fixture.
function grammarFor(path: string): Make | undefined {
  switch (extname(path)) {
    case '.json':
      return () => makeJson()
    case '.jsonl':
      return () => makeJsonl()
    case '.yaml':
      return () => new Tabnas().use(jsonic).use(Yaml)
    case '.csv':
    case '.tsv':
      return () => makeCsv()
    default:
      return undefined
  }
}

type Outcome = { ok: true; text: string } | { ok: false; fail: any }

function attempt(work: () => string): Outcome {
  try {
    return { ok: true, text: work() }
  } catch (fail) {
    return { ok: false, fail }
  }
}

// Where the worked example's records are.
function recordsSelector(): Selector {
  return Selector.root().property('response').property('payload').property('deep').property('records').eachIndex()
}

// The components the library's text is held to: transduce's table
// transducer over the worked example's binding (its columns from the
// metadata, or inferred from the first row) and render's CSV renderer in
// its default dialect, the events replayed through the source's guard; the
// bytes, or the failure thrown.
function components(evs: any[], limits: Limits, infer: boolean): string {
  const writer = new BytesWriter()
  const csv = renderers.csv(renderers.writeOut(writer))
  const schema = infer
    ? Schema.infer()
    : Schema.fromMetadata(Selector.root().property('response').property('metadata').property('fields'))
  const table = new TableFromJson({ schema, rows: recordsSelector() }, limits, 'reject', new Metrics(), csv)
  replay(evs, new Guarded(table, limits, new AbortFlag(), new Metrics()))
  return writer.text()
}

// One document through the library's text and through the components: the
// same bytes, or the same code; a report of the difference otherwise.
function agreesWithComponents(name: string, program: Program, evs: any[], infer: boolean): string | undefined {
  const a = attempt(() => replayed(program, evs))
  const b = attempt(() => components(evs, Limits.default(), infer))
  if (a.ok && b.ok) {
    return a.text === b.text
      ? undefined
      : `${name}: the bytes differ\n  text:       ${JSON.stringify(a.text)}\n  components: ${JSON.stringify(b.text)}`
  }
  if (!a.ok && !b.ok) {
    return a.fail?.code === b.fail?.code
      ? undefined
      : `${name}: the codes differ\n  text:       ${a.fail}\n  components: ${b.fail}`
  }
  if (a.ok && !b.ok) return `${name}: the text produced ${JSON.stringify(a.text)}, the components failed: ${b.fail}`
  return `${name}: the components produced ${JSON.stringify((b as any).text)}, the text failed: ${(a as any).fail}`
}

// A document to compare: its name, the grammar that reads it, its text.
type Document = [string, Make, string]

// One outcome: the bytes, or the code; `expected` is that outcome.
function expectOutcome(
  name: string,
  program: Program,
  evs: any[],
  limits: Limits,
  expected: { ok: string } | { code: Code },
): void {
  const got = attempt(() => replayed(program, evs, undefined, limits))
  if ('ok' in expected) {
    assert.ok(got.ok, `${name}: expected ${JSON.stringify(expected.ok)}, got ${(got as any).fail}`)
    assert.equal(got.text, expected.ok, name)
    return
  }
  assert.ok(!got.ok, `${name}: expected ${expected.code}, got ${JSON.stringify((got as any).text)}`)
  assert.equal(got.fail.code, expected.code, `${name}: ${got.fail}`)
}

const json = (text: string): any[] => events(makeJson(), text) as any[]

describe('the standard library on the stages', () => {
  // Every transduce fixture a grammar here reads, plus the generated
  // documents in every shape: the text writes the components' bytes, or
  // fails with their code.
  it('the library writes what the components write for every fixture and generated document', () => {
    const program = compile(PROGRAM, 'export.alc')
    const documents: Document[] = []
    const entries = readdirSync(FIXTURES)
      .map((f) => join(FIXTURES, f))
      .sort()
    for (const path of entries) {
      const make = grammarFor(path)
      if (undefined === make) continue
      documents.push([path, make, readFileSync(path, 'utf8')])
    }
    for (const n of [0, 1, 2, 3, 50]) documents.push([`recordsJson(${n})`, () => makeJson(), recordsJson(n)])
    documents.push(['recordsYaml(3)', () => new Tabnas().use(jsonic).use(Yaml), recordsYaml(3)])
    documents.push(['recordsJsonl(3)', () => makeJsonl(), recordsJsonl(3)])
    documents.push(['recordsCsv(3)', () => makeCsv(), recordsCsv(3)])
    // Cells of every kind, in the worked example's shape: null, missing
    // members (a failure under the standard options), nested containers,
    // booleans, quotes, delimiters, line breaks and non-ASCII text.
    const shaped = (records: string) =>
      `{"response":{"metadata":${METADATA},"payload":{"deep":{"records":[${records}]}}}}`
    for (const [name, records] of [
      ['nulls', '{"id":null,"person":{"name":null},"account":{"balance":null}}'],
      ['missing', '{"id":1,"person":{},"account":{"balance":2}}'],
      ['containers', '{"id":[1,2.50],"person":{"name":{"first":"A","last":"B"}},"account":{"balance":{}}}'],
      ['booleans', '{"id":true,"person":{"name":false},"account":{"balance":0}}'],
      ['quotes', '{"id":"say \\"hi\\"","person":{"name":"a,b"},"account":{"balance":"line\\r\\nbreak"}}'],
      ['unicode', '{"id":"caf\\u00e9","person":{"name":"日本"},"account":{"balance":"\\ud83d\\ude00"}}'],
      ['lexemes', '{"id":1e2,"person":{"name":"x"},"account":{"balance":-0.0}}'],
      ['big', '{"id":12345678901234567890123,"person":{"name":"x"},"account":{"balance":1E+2}}'],
      [
        'two rows out of order',
        '{"account":{"balance":1},"id":2,"person":{"name":"z"}},{"id":3,"person":{"name":"y"},"account":{"balance":4}}',
      ],
    ]) {
      documents.push([name, () => makeJson(), shaped(records)])
    }
    // The order contract and the absent shapes.
    documents.push([
      'metadata after rows',
      () => makeJson(),
      `{"response":{"payload":{"deep":{"records":[${record(1)}]}},"metadata":${METADATA}}}`,
    ])
    documents.push(['no metadata', () => makeJson(), `{"response":{"payload":{"deep":{"records":[${record(1)}]}}}}`])
    documents.push(['no records', () => makeJson(), `{"response":{"metadata":${METADATA}}}`])
    documents.push(['metadata not an array', () => makeJson(), '{"response":{"metadata":{"fields":{}}}}'])
    documents.push([
      'descriptor with a bad segment',
      () => makeJson(),
      '{"response":{"metadata":{"fields":[{"title":"a","path":[true]}]}}}',
    ])
    documents.push(['row that is not an object', () => makeJson(), shaped('1')])

    const total = documents.length
    const failures: string[] = []
    let skipped = 0
    documents.forEach(([name, make, text], i) => {
      if (0 === i % 10) console.log(`sweep: ${i} of ${total} (${Math.floor((i * 100) / total)}%)`)
      const evs = events(make(), text)
      if (undefined === evs) {
        // The grammar refused the document: nothing reached a program.
        skipped++
        return
      }
      const report = agreesWithComponents(name, program, evs, false)
      if (undefined !== report) failures.push(report)
    })
    console.log(`sweep: ${total} of ${total} (100%), ${skipped} unreadable`)
    assert.ok(total - skipped > 20, 'enough documents were read')
    assert.deepStrictEqual(failures, [], `${failures.length} document(s) differ`)
  })

  // The other renderings the text offers: the table as records, rendered
  // as JSON (`records` then `json`), and a `csv` whose options record
  // names another dialect, which the text writes as named.
  it('the other renderings write as their programs say', () => {
    const program = compile(PROGRAM.replace('    csv csv-options\n', '    records\n    json\n'), 'records.alc')
    const dialect = compile(
      PROGRAM.replace(
        '    csv csv-options\n',
        '    csv options\n\ndef options\n  record\n    entry :delimiter ";"\n    entry :newline "\\n"\n    entry :header false\n    entry :null-text "NULL"\n    entry :missing "-"\n',
      ),
      'dialect.alc',
    )
    const three = json(recordsJson(3))
    const rows = JSON.parse(replayed(program, three))
    assert.equal(rows.length, 3)
    assert.equal(rows[2]['Full name'], 'Person number 2')
    const csv = replayed(dialect, three)
    const lines = csv.split('\n').filter((l) => '' !== l)
    assert.equal(lines.length, 3, csv)
    assert.ok(lines.every((l) => 2 === l.split(';').length - 1), csv)
    const sparse = json(
      `{"response":{"metadata":${METADATA},"payload":{"deep":{"records":[{"id":null,"person":{},"account":{"balance":"a;b"}}]}}}}`,
    )
    assert.equal(replayed(dialect, sparse), '"NULL";"-";"a;b"\n')
    // A missing cell is no member of the record.
    assert.equal(replayed(program, sparse), '[{"Identifier":null,"Balance":"a;b"}]\n')
  })

  // The library's `csv` validates the table events it renders as render's
  // CSV renderer does (spec 13.2: the protocol validator is not omitted),
  // so a program that produces its own table events, or a document or an
  // options record the renderer refuses, fails with the renderer's code,
  // and a sound one prints the renderer's bytes.
  it('the library csv validates what the renderer validates', () => {
    const items = json('{"items":[{"n":"a","v":1},{"n":"b\\"q","v":-2},{"n":"c","v":null}],"tail":"t"}')
    const limits = Limits.default()
    const label = (l: string) => `(record (entry :label "${l}"))`
    const table = (step: string, finish: string) =>
      `def export [input] (csv csv-options (scan-emit no-schema ${step} ${finish} (select (path "items" each-index) input)))`
    // One schema on the first item, a row per item: the sound shape.
    const first = (schema: string, row: string) =>
      `(fn [s x] (transition (ready []) (if (is-ready s) [(row ${row})] [(schema ${schema}) (row ${row})])))`
    const n = label('N')
    const v = label('V')
    const cases: Array<[string, string, { ok: string } | { code: Code }]> = [
      [
        'sound',
        table(first(`[${n} ${v}]`, '[(get :n x) (get :v x)]'), '(fn [s] [table-end])'),
        { ok: '"N","V"\r\n"a","1"\r\n"b""q","-2"\r\n"c",""\r\n' },
      ],
      ['rows without a schema', table('(fn [s x] (transition s [(row [(get :n x)])]))', '(fn [s] [table-end])'), { code: 'PROTOCOL_ORDER_ERROR' }],
      ['a schema per item', table(`(fn [s x] (transition s [(schema [${n}])]))`, '(fn [s] [table-end])'), { code: 'PROTOCOL_ORDER_ERROR' }],
      ['a row wider than the schema', table(first(`[${n}]`, '[(get :n x) (get :v x)]'), '(fn [s] [table-end])'), { code: 'PROTOCOL_ORDER_ERROR' }],
      ['no table-end', table(first(`[${n}]`, '[(get :n x)]'), '(fn [s] [])'), { code: 'PROTOCOL_ORDER_ERROR' }],
      ['two table-ends', table(first(`[${n}]`, '[(get :n x)]'), '(fn [s] [table-end table-end])'), { code: 'PROTOCOL_ORDER_ERROR' }],
      [
        'an item that is not a table event',
        'def export [input] (csv csv-options (map (fn [x] (get :n x)) (select (path "items" each-index) input)))',
        { code: 'PROTOCOL_ORDER_ERROR' },
      ],
      [
        'no items at all',
        'def export [input] (csv csv-options (scan-emit no-schema (fn [s x] (transition s [x])) (fn [s] []) (select (path "none" each-index) input)))',
        { code: 'PROTOCOL_ORDER_ERROR' },
      ],
      ['a schema of no columns', table(first('[]', '[]'), '(fn [s] [table-end])'), { code: 'TARGET_VALUE_UNREPRESENTABLE' }],
      ['a column without a label', table(first('[(record (entry :x "N"))]', '[(get :n x)]'), '(fn [s] [table-end])'), { code: 'MISSING_VALUE' }],
      ['a label that is a record', table(first('[(record (entry :label (record)))]', '[(get :n x)]'), '(fn [s] [table-end])'), { code: 'INPUT_INVALID' }],
    ]
    for (const [name, src, expected] of cases) {
      expectOutcome(name, compile(src, 'table.alc'), items, limits, expected)
    }

    // The worked example over a document whose metadata is empty: a table
    // of no columns has no CSV form.
    const program = compile(PROGRAM, 'export.alc')
    expectOutcome(
      'no columns',
      program,
      json('{"response":{"metadata":{"fields":[]},"payload":{"deep":{"records":[{"id":1}]}}}}'),
      limits,
      { code: 'TARGET_VALUE_UNREPRESENTABLE' },
    )

    // A delimiter no CSV reader could take is refused before anything runs
    // (`csv-table`, as the renderer refuses it).
    const records = json(RECORDS)
    for (const delimiter of ['\\"', '\\n', '\\r']) {
      const src = PROGRAM.replace(
        '    csv csv-options\n',
        `    csv (record (entry :delimiter "${delimiter}") (entry :newline "\\r\\n") (entry :header true) (entry :null-text "") (entry :missing :error))\n`,
      )
      expectOutcome(`delimiter ${delimiter}`, compile(src, 'delimiter.alc'), records, limits, {
        code: 'TARGET_VALUE_UNREPRESENTABLE',
      })
    }

    // A column function that answers something other than a record fails
    // as `get` does.
    const keyword = compile(
      'def b (record (entry :columns (path "m")) (entry :rows (path "r" each-index)) (entry :column (fn [d] :oops)))\ndef export [input] (csv csv-options (table-from-json b input))',
      'keyword.alc',
    )
    expectOutcome('a column that is a keyword', keyword, json('{"m":[{"title":"t","path":["a"]}],"r":[{"a":"x"}]}'), limits, {
      code: 'DSL_TYPE_ERROR',
    })
  })

  // The library's table holds the scopes it captures to the host's limits
  // (the metadata under `max_metadata_bytes`, each row under
  // `max_record_bytes`, at most `max_columns` columns), so under the host's
  // own limits it fails naming the limit, or prints, and counts its rows.
  it('the limits hold', () => {
    const program = compile(PROGRAM, 'export.alc')
    const records = json(RECORDS)
    const cases: Array<[string, Limits, { ok: string } | { code: Code }]> = [
      ['defaults', Limits.default(), { ok: EXPECTED_CSV }],
      ['max_record_bytes', Limits.with({ max_record_bytes: 100 }), { code: 'RESOURCE_LIMIT_EXCEEDED' }],
      ['max_metadata_bytes', Limits.with({ max_metadata_bytes: 50 }), { code: 'RESOURCE_LIMIT_EXCEEDED' }],
      ['max_columns', Limits.with({ max_columns: 2 }), { code: 'RESOURCE_LIMIT_EXCEEDED' }],
      ['max_capture_bytes decides neither table', Limits.with({ max_capture_bytes: 100 }), { ok: EXPECTED_CSV }],
    ]
    for (const [name, limits, expected] of cases) expectOutcome(name, program, records, limits, expected)
    // A cell that is a vector is its compact JSON text, one scalar of the
    // output, held to max_scalar_bytes.
    const containers = json(RECORDS.replace('"id":123', '"id":[1,2,3,4,5,6]'))
    expectOutcome('a container cell under max_scalar_bytes', program, containers, Limits.with({ max_scalar_bytes: 12 }), {
      code: 'RESOURCE_LIMIT_EXCEEDED',
    })
    expectOutcome('a container cell within it', program, containers, Limits.with({ max_scalar_bytes: 13 }), {
      ok: '"Identifier","Full name","Balance"\r\n"[1,2,3,4,5,6]","Alice","50.25"\r\n"456","Bob","72"\r\n',
    })
    const metrics = new Metrics()
    replayed(program, records, undefined, Limits.default(), metrics)
    assert.equal(metrics.rows, 2)
  })

  // A program over an inferred binding (`:columns :infer`): a root array of
  // rows, written as CSV with a missing cell as an empty field. The columns
  // are the first row's keys, in its order, each reading its own key; the
  // library's text infers them with `keys`, over every shape a first row
  // and a later row can take, and holds them to the limits. Over the worked
  // example's documents it writes what transduce's `Schema.infer()` and
  // render's CSV renderer write.
  it("an inferred binding infers the first row's columns", () => {
    const INFERRED =
      'def options\n  record\n    entry :delimiter ","\n    entry :newline "\\r\\n"\n    entry :header true\n    entry :null-text ""\n    entry :missing ""\n\n' +
      'def rows-binding\n  record\n    entry :columns :infer\n    entry :rows (path each-index)\n\n' +
      'def export [input]\n  pipe input\n    table-from-json rows-binding\n    csv options\n'
    const program = compile(INFERRED, 'inferred.alc')
    const ok = (name: string, text: string, want: string) => expectOutcome(name, program, json(text), Limits.default(), { ok: want })
    const err = (name: string, text: string, code: Code) => expectOutcome(name, program, json(text), Limits.default(), { code })
    ok('the first row\'s keys, in its order', '[{"b":1,"a":"x"},{"a":"y","b":2}]', '"b","a"\r\n"1","x"\r\n"2","y"\r\n')
    ok('a key a later row lacks is a missing cell', '[{"a":1,"b":2},{"a":3}]', '"a","b"\r\n"1","2"\r\n"3",""\r\n')
    ok('a key only a later row has is no column', '[{"a":1},{"c":3,"a":2}]', '"a"\r\n"1"\r\n"2"\r\n')
    ok(
      'cells of every kind',
      '[{"s":"q\\"x","n":1.5,"t":true,"z":null,"o":{"k":[1,2]}}]',
      '"s","n","t","z","o"\r\n"q""x","1.5","true","","{""k"":[1,2]}"\r\n',
    )
    ok('a key that spells an index names a member', '[{"0":"zero","1":"one"}]', '"0","1"\r\n"zero","one"\r\n')
    ok('a later row that is not an object has missing cells', '[{"a":1},2]', '"a"\r\n"1"\r\n""\r\n')
    // A first row of another kind than an object: a scalar is one `value`
    // column, the row itself; an array's columns are its positions. A later
    // row projects through the first row's paths, so an object under
    // positional columns is a missing cell, and under a `value` column its
    // compact JSON text.
    ok('a first row that is a scalar', '[1,{"a":1}]', '"value"\r\n"1"\r\n"{""a"":1}"\r\n')
    ok('a first row that is an array', '[[1,"x"],{"a":1},[2]]', '"0","1"\r\n"1","x"\r\n"",""\r\n"2",""\r\n')
    // A table of no columns, from no rows or from an empty first row, is
    // one the CSV renderer refuses, as it refuses any.
    err('no rows', '[]', 'TARGET_VALUE_UNREPRESENTABLE')
    err('an empty first row', '[{},{"a":1}]', 'TARGET_VALUE_UNREPRESENTABLE')
    const three = json('[{"a":1,"b":2,"c":3}]')
    expectOutcome('max_columns holds the inferred columns', program, three, Limits.with({ max_columns: 2 }), {
      code: 'RESOURCE_LIMIT_EXCEEDED',
    })
    // Three one-byte names take 16 + 3 * (16 + 1) = 67 bytes as the
    // labels' array; the library's state holding them is larger still.
    expectOutcome('max_metadata_bytes holds the inferred columns', program, three, Limits.with({ max_metadata_bytes: 40 }), {
      code: 'RESOURCE_LIMIT_EXCEEDED',
    })
    expectOutcome('max_record_bytes holds each row', program, three, Limits.with({ max_record_bytes: 8 }), {
      code: 'RESOURCE_LIMIT_EXCEEDED',
    })
    // The column count holds where the schema is built, so it holds when
    // the table events reach no renderer that would check them.
    for (const [name, binding] of [
      ['an inferred table\'s events as items', '(record (entry :columns :infer) (entry :rows (path each-index)))'],
      [
        'a described table\'s events as items',
        '(record (entry :columns (path "m")) (entry :rows (path "r" each-index)) (entry :column (fn [d] (record (entry :label d) (entry :source (path d))))))',
      ],
    ]) {
      const items = compile(`def b ${binding}\ndef export [input] (join "" (map (fn [e] "x") (table-from-json b input)))`, 'items.alc')
      const evs = name.startsWith('an inferred') ? three : json('{"m":["a","b","c"],"r":[{"a":1,"b":2,"c":3}]}')
      expectOutcome(name, items, evs, Limits.with({ max_columns: 2 }), { code: 'RESOURCE_LIMIT_EXCEEDED' })
      expectOutcome(name, items, evs, Limits.default(), { ok: 'xxx' })
    }
    // The worked example's documents, bound by inference from their rows.
    const api = compile(
      INFERRED.replace('(path each-index)', '(path "response" "payload" "deep" "records" each-index)'),
      'inferred-api.alc',
    )
    const failures: string[] = []
    for (const n of [0, 1, 2, 3, 50]) {
      const report = agreesWithComponents(`recordsJson(${n})`, api, json(recordsJson(n)), true)
      if (undefined !== report) failures.push(report)
    }
    assert.deepStrictEqual(failures, [])
  })

  // Each row counts once in `metrics.rows`, by the last table stage it
  // passes: the adapter to a renderer, or a `csv-table` whose rows reach no
  // later table stage. A `map`, a `filter` or a `scan-emit` between two
  // table stages hands the count on, so a row a filter drops is never
  // counted, and rows that only ever become a text of items are not rows of
  // any table.
  it('each row counts once across composed table stages', () => {
    const tail = (t: string) => PROGRAM.replace('    csv csv-options\n', t)
    const identity = '    map (fn [e] e)\n'
    const dropRows = '    filter (fn [e] (match e (case (row cells) false) (case _ true)))\n'
    const passScan = '    scan-emit null (fn [s e] (transition s [e])) (fn [s] []) \n'
    const asText = '    map (fn [e] "x")\n    join ","\n'
    const cases: Array<[string, string, 'csv' | 'json' | undefined, number]> = [
      ['csv', tail('    csv csv-options\n'), undefined, 2],
      ['csv-table as the result', tail('    csv-table csv-options\n'), undefined, 2],
      ['csv-table as the result, as json', tail('    csv-table csv-options\n'), 'json', 2],
      ['csv over csv-table', tail('    csv-table csv-options\n    csv csv-options\n'), undefined, 2],
      ['records over csv-table', tail('    csv-table csv-options\n    records\n    json\n'), undefined, 2],
      ['csv-table twice', tail('    csv-table csv-options\n    csv-table csv-options\n    csv csv-options\n'), undefined, 2],
      [
        'the library csv with its own dialect',
        tail(
          '    csv opts\n\ndef opts\n  record\n    entry :delimiter "||"\n    entry :newline "\\r\\n"\n    entry :header true\n    entry :null-text ""\n    entry :missing :error\n',
        ),
        undefined,
        2,
      ],
      ['a map, then the program\'s csv', tail(`${identity}    csv csv-options\n`), undefined, 2],
      ['a map, csv-table, csv', tail(`${identity}    csv-table csv-options\n    csv csv-options\n`), undefined, 2],
      ['csv-table, a map, csv', tail(`    csv-table csv-options\n${identity}    csv csv-options\n`), undefined, 2],
      ['a map, the host\'s renderer', tail(identity), 'csv', 2],
      ['a map, the host\'s json', tail(identity), 'json', 2],
      ['a filter that keeps every row, the host\'s renderer', tail('    filter (fn [e] true)\n'), 'csv', 2],
      ['a scan that passes each event on, then csv', tail(`${passScan}    csv csv-options\n`), undefined, 2],
      ['a filter that drops every row, then csv', tail(`${dropRows}    csv csv-options\n`), undefined, 0],
      ['a filter that drops every row, the host\'s renderer', tail(dropRows), 'csv', 0],
      ['the table\'s events as a text: no table stage', tail(asText), undefined, 0],
      ['csv-table\'s events as a text', tail(`    csv-table csv-options\n${asText}`), undefined, 2],
    ]
    const records = json(RECORDS)
    for (const [name, src, render, rows] of cases) {
      const program = compile(src, 'rows.alc')
      const metrics = new Metrics()
      try {
        replayed(program, records, render, Limits.default(), metrics)
      } catch (fail) {
        assert.fail(`${name}: ${fail}`)
      }
      assert.equal(metrics.rows, rows, name)
    }
  })

  // Where the text and the components answer apart, pinned so a change to
  // either is seen. Metadata selected twice is the text's own refusal,
  // `fail`, which is INPUT_INVALID, where transduce's table transducer
  // calls the same document an order violation, a table having one schema.
  // A descriptor without a title is a column without a label to the text's
  // `column-from-meta`, which the table refuses as a missing value, where
  // transduce's `columnFromMeta` refuses the descriptor itself. The
  // standard shapes reach neither. And a numeric title renders by its
  // lexeme, one label policy serving every table.
  it('where the text and the components answer apart', () => {
    const twice = compile(
      PROGRAM.replace('      path "response" "metadata" "fields"\n', '      path "response" "metadata" each-index\n'),
      'twice.alc',
    )
    const doc = `{"response":{"metadata":[${'[{"title":"a","path":["id"]}]'},${'[{"title":"b","path":["id"]}]'}],"payload":{"deep":{"records":[${record(1)}]}}}}`
    const evs = json(doc)
    const fail = attemptFail(() => replayed(twice, evs))
    assert.equal(fail.code, 'INPUT_INVALID', String(fail))
    assert.match(String(fail.message), /metadata selected more than once/i, String(fail))
    assert.equal(attemptFail(() => components(evs, Limits.default(), false)).code, 'INPUT_ORDER_VIOLATION')
    const program = compile(PROGRAM, 'export.alc')
    // A descriptor without a title: the text's column-from-meta makes a
    // column whose label is missing, which the table refuses as a missing
    // value; transduce's columnFromMeta refuses the descriptor itself.
    const untitled = json('{"response":{"metadata":{"fields":[{"path":["a"]}]}}}')
    assert.equal(attemptFail(() => replayed(program, untitled)).code, 'MISSING_VALUE')
    assert.equal(attemptFail(() => components(untitled, Limits.default(), false)).code, 'INPUT_INVALID')
    const numeric = json(
      `{"response":{"metadata":{"fields":[{"title":42,"path":["id"]}]},"payload":{"deep":{"records":[${record(1)}]}}}}`,
    )
    expectOutcome('a numeric title', program, numeric, Limits.default(), { ok: '"42"\r\n"1"\r\n' })
  })

  // The events end with `end`, as every source promises; a recording that
  // does not is a source defect, and the sink says so rather than dropping
  // the output silently: nothing is flushed.
  it('a stream without end writes nothing', () => {
    const program = compile(PROGRAM, 'export.alc')
    const evs = json(recordsJson(2))
    assert.deepStrictEqual(evs.pop(), Ev.end)
    const writer = new BytesWriter()
    const sink = program.sink(writer, undefined, Limits.default(), new Metrics())
    replay(evs, sink)
    assert.equal(writer.text(), '')
    // And the end then completes it.
    sink.event(Ev.end)
    assert.notEqual(writer.text(), '')
  })
})

function attemptFail(work: () => unknown): any {
  try {
    work()
  } catch (fail) {
    return fail
  }
  throw new Error('expected a failure')
}

// The rule of transduce.tsv, the oracle a part has: what a part wrote, read
// back from its JSON, is the walk of the grammar's own value of the
// document, a number's lexeme aside. A member the part streamed twice is
// kept once by the value, so such an output has no oracle and passes.
function isTheWalk(written: string, value: unknown): string | undefined {
  const collect = new Collect(new DatumBuilder(Number.POSITIVE_INFINITY, 'max_capture_bytes', 'reject'))
  try {
    jsonInto(written, Limits.default(), new Metrics(), collect)
  } catch (err: any) {
    if ('DUPLICATE_MEMBER' === err?.code) return undefined
    return `the output ${JSON.stringify(written)} does not read back as JSON: ${String(err)}`
  }
  const read = collect.builder.take()
  if (undefined === read) return `the output ${JSON.stringify(written)} holds no value`
  const a = JSON.stringify(plainEvents(read))
  const b = JSON.stringify(plainEvents(fromTabnas(value)))
  return a === b ? undefined : `the part's events are not the walk of the grammar's value\n  written: ${a}\n  walked:  ${b}`
}

// A part over every document: where the grammar accepts the document and
// `oracle` says the part writes its tree, the output is the walk of the
// grammar's value; where the grammar refuses it, the part fails, with the
// grammar's refusal or with a reason of its own it met first; and any
// other failure is one of the catalogue's, the part's own reason. The
// failures, for the test to report.
function partOver(specName: string, program: Program, make: Make, documents: Array<[string, string]>, oracle: boolean): string[] {
  const failures: string[] = []
  for (const [name, text] of documents) {
    const label = `${specName}, ${name}`
    const out = attempt(() => {
      const writer = new BytesWriter()
      new ParserEventSource(make(), text).run(program.parserSink(writer, 'json'))
      return writer.text()
    })
    let value: unknown
    let refused = false
    try {
      value = make().parse(text)
    } catch {
      refused = true
    }
    if (!out.ok) {
      const code = out.fail?.code
      if (refused) {
        // The part fails: with the grammar's refusal, or with a reason of
        // its own it met before the refusal arrived.
        if (!Code.ALL.includes(code)) failures.push(`${label}: the grammar refuses the document, the part fails outside the catalogue: ${out.fail}`)
      } else if (oracle || !Code.ALL.includes(code)) {
        failures.push(`${label}: ${out.fail}`)
      }
    } else if (refused) {
      failures.push(`${label}: wrote ${JSON.stringify(out.text)} of a document the grammar refuses`)
    } else if (oracle) {
      const report = isTheWalk(out.text, value)
      if (undefined !== report) failures.push(`${label}: ${report}`)
    }
  }
  return failures
}

// The specs the transduce-tree sweep runs: the json part's, and specs that
// take a close pass's value, hand a container on through a replaced rule,
// refuse a key that is not a string, announce a named key, yield a rule's
// text and name a member inside a rule's own container.
const TREE_SPECS: Array<[string, string]> = [
  [
    "the json part's spec",
    'record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar 0))',
  ],
  [
    "a close pass's value, keyed by name",
    'record\n    entry :map (record (entry :container :object))\n    entry :list (record (entry :container :array))\n    entry :pair (record (entry :key 0))\n    entry :val (record (entry :scalar :value))',
  ],
  [
    'a container handed on through a replaced rule',
    'record\n    entry :list (record (entry :container :array))\n    entry :elem (record (entry :container :array))\n    entry :val (record (entry :scalar 0))',
  ],
  ['a key whose token is not a string', 'record\n    entry :val (record (entry :key 0) (entry :scalar 0))'],
  [
    'a named key on every value',
    'record\n    entry :list (record (entry :container :array))\n    entry :val (record (entry :key "v") (entry :scalar 0))',
  ],
  [
    "a pair's value as its text",
    'record\n    entry :map (record (entry :container :object))\n    entry :pair (record (entry :key 0) (entry :text true))',
  ],
  [
    "a container's own member and its text",
    'record\n    entry :list (record (entry :container :object) (entry :member "items") (entry :text true))\n    entry :pair (record (entry :member 0))',
  ],
]

describe("the library's transduce-tree", () => {
  // Over every JSON fixture of transduce's, the generated documents and the
  // documents below, for every spec: the json part writes the walk of the
  // grammar's value, and a document the grammar refuses fails as the
  // grammar refuses it (rs/tests/stdlib_test.rs).
  it("writes each document's tree", () => {
    const documents: Array<[string, string]> = []
    for (const f of readdirSync(FIXTURES).sort()) {
      if ('.json' === extname(f)) documents.push([f, readFileSync(join(FIXTURES, f), 'utf8')])
    }
    for (const n of [0, 1, 2, 50]) documents.push([`recordsJson(${n})`, recordsJson(n)])
    for (const text of [
      '1',
      '2.50',
      '-0',
      '1E+2',
      '"s"',
      'null',
      '[]',
      '{}',
      '[[[]],[{}]]',
      '{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}',
      '{"é":"日本","a\\"b":"\\u0000"}',
      '{"a":1,"a":2}',
      '[1,',
      '{"a" 1}',
    ]) {
      documents.push([JSON.stringify(text), text])
    }
    const failures: string[] = []
    let compared = 0
    for (const [specName, spec] of TREE_SPECS) {
      const program = compileWith(`def spec\n  ${spec}\n\ndef export [input] (transduce-tree spec input)\n`, 'tree.alc', 'ParserEvents/1')
      failures.push(...partOver(specName, program, () => makeJson(), documents, "the json part's spec" === specName))
      compared += documents.length
    }
    assert.deepStrictEqual(failures, [])
    assert.ok(100 < compared, `${compared} documents compared`)
  })

  // A `:text` rule's text under a small `max_scalar_bytes`: the library's
  // `string-join` holds it at each of its joins, so at each bound from one
  // that refuses every text to one that refuses none the part writes the
  // document (one its spec is sound for) or refuses it naming
  // `max_scalar_bytes`, some of the refusals the texts' own, not the
  // guard's; the widest bound refuses none.
  it("holds a :text rule's text to max_scalar_bytes", () => {
    // Each spec over documents it is sound for: a pair's text is a scalar
    // after the pair's key, so no value may be an object (whose own
    // container would stand where the text goes), and a list's member and
    // text are announced inside the list's own object, so no pair, and no
    // list, may lie outside a flat array.
    const textSpec = (name: string): string => {
      const found = TREE_SPECS.find(([n, spec]) => n === name && spec.includes(':text'))
      assert.ok(found, name)
      return found[1]
    }
    const specs: Array<[string, string, string[]]> = [
      [
        "a pair's value as its text",
        textSpec("a pair's value as its text"),
        [
          '{"a":[10,20]}',
          '{"a":"xyz","bb":[1,[22,333],"x"]}',
          '{"a":1e21,"b":true,"c":null,"d":[]}',
          '{"é":"日本"}',
          '[1,[22,333],"x",{"é":"日本"}]',
          '{"a":[1,2.50,"x"],"c":[],"e":1e21,"f":true}',
        ],
      ],
      [
        "a container's own member and its text",
        textSpec("a container's own member and its text"),
        ['[1,2]', '[]', '[1,"x",true,null,2.50]', '["日本","é"]', '[1,2,3,4,5,6,7,8,9,10,11,12]', '[-0,1E+2,"a,b"]'],
      ],
    ]
    const failures: string[] = []
    let textsRefused = 0
    for (const [specName, spec, documents] of specs) {
      const program = compileWith(`def spec\n  ${spec}\n\ndef export [input] (transduce-tree spec input)\n`, 'tree.alc', 'ParserEvents/1')
      for (const text of documents) {
        for (const max of [1, 3, 4, 5, 7, 8, 11, 16, 64]) {
          const limits = Limits.with({ max_scalar_bytes: max })
          const writer = new BytesWriter()
          const out = attempt(() => {
            new ParserEventSource(makeJson(), text).run(program.parserSink(writer, 'json', limits))
            return writer.text()
          })
          if (out.ok) {
            assert.notEqual(out.text, '', `${specName}, ${text}, ${max}: wrote nothing`)
            continue
          }
          const fail = out.fail
          if ('RESOURCE_LIMIT_EXCEEDED' !== fail?.code || 'max_scalar_bytes' !== fail?.limit?.name) {
            failures.push(`${specName}, ${text}, max_scalar_bytes ${max}: ${fail}`)
            continue
          }
          assert.notEqual(max, 64, `${specName}, ${text}: refused at the widest bound: ${fail}`)
          if (String(fail.message).startsWith('string-join:')) textsRefused++
        }
      }
    }
    assert.deepStrictEqual(failures, [])
    assert.ok(10 < textsRefused, `${textsRefused} texts refused at the bound`)
  })
})

// tabnas-csv's own test/spec corpus, transduce's CSV fixtures, the
// generated records and the documents below: what the records tests read.
function csvDocuments(): Array<[string, string]> {
  const corpus = vendored('csv', 'test', 'spec')
  const documents: Array<[string, string]> = []
  let files
  try {
    files = loadSpecDir(corpus)
  } catch (err) {
    throw new Error(`tabnas-csv's corpus is vendored, at ${corpus}: ${String(err)}`)
  }
  for (const file of files) {
    for (const row of file.rows) documents.push([`${row.file}:${row.line}`, row.unescNamed('input')])
  }
  for (const f of readdirSync(FIXTURES).sort()) {
    if ('.csv' === extname(f)) documents.push([f, readFileSync(join(FIXTURES, f), 'utf8')])
  }
  for (const n of [0, 1, 2, 50]) documents.push([`recordsCsv(${n})`, recordsCsv(n)])
  for (const text of [
    '',
    'a,b\n',
    'a,b,c\n1,,3\n,5,\n',
    'a,b,c\n1,2\n4\n',
    'a,b\n1,2,3,4\n',
    'a,b\n"x,y","line\nbreak"\n"q""q",z\n',
    'a,b\r\n1,2\r\n',
    'a,a\n1,2\n',
    ' a , b \n 1 , 2 \n',
    'a,b,\n1,2,\n',
    'a,"b\n',
  ]) {
    documents.push([JSON.stringify(text), text])
  }
  return documents
}

const RECORDS_SPEC =
  'record\n    entry :document "csv"\n    entry :record "record"\n    entry :cell "val"\n    entry :empty "empty"\n    entry :header true\n    entry :unnamed "field~"'

describe("the library's transduce-records", () => {
  // Over tabnas-csv's corpus, transduce's CSV fixtures, the generated
  // records and documents that hold empty cells, short and long records, a
  // header that repeats a name and documents the grammar refuses, for csv's
  // part's spec and for specs without a header, without an empty group and
  // with rules no pass names: csv's part writes the walk of the grammar's
  // value (a header that names a column twice streams both cells, where the
  // value keeps the last: no oracle there), and a document the grammar
  // refuses fails as the grammar refuses it, for every spec.
  it("writes each document's records", () => {
    const specs: Array<[string, string]> = [
      ["csv's part", RECORDS_SPEC],
      [
        'no header, null for an empty cell',
        'record\n    entry :document "csv"\n    entry :record "record"\n    entry :cell "val"\n    entry :empty "empty"\n    entry :fill null',
      ],
      [
        'a header and no empty group',
        'record\n    entry :document "csv"\n    entry :record "record"\n    entry :cell "val"\n    entry :header true',
      ],
      ['rules no pass names', 'record\n    entry :document "doc"\n    entry :record "rec"\n    entry :cell "cell"\n    entry :header true'],
    ]
    const documents = csvDocuments()
    const failures: string[] = []
    let compared = 0
    for (const [specName, spec] of specs) {
      const program = compileWith(
        `def spec\n  ${spec}\n\ndef export [input] (transduce-records spec input)\n`,
        'records.alc',
        'ParserEvents/1',
      )
      failures.push(...partOver(specName, program, () => makeCsv(), documents, "csv's part" === specName))
      compared += documents.length
    }
    assert.deepStrictEqual(failures, [])
    assert.ok(200 < compared, `${compared} documents compared`)
  })

  // The state under a small `max_metadata_bytes`: the library's scans hold
  // the header's names and a record's cells by the transduce measure, so at
  // each bound from one that refuses the state before any pass to one that
  // refuses none the part writes the document, the same bytes at every
  // bound that lets it through, or refuses it naming `max_metadata_bytes`.
  it('holds the state to max_metadata_bytes', () => {
    const program = compileWith(
      `def spec\n  ${RECORDS_SPEC}\n\ndef export [input] (transduce-records spec input)\n`,
      'records.alc',
      'ParserEvents/1',
    )
    const text = 'identifier,full name,balance\n1,Ann,2.50\n2,,\n3,Bob,7,extra\n'
    let refused = 0
    let written: string | undefined
    for (let max = 8; max <= 220; max += 4) {
      const limits = Limits.with({ max_metadata_bytes: max })
      const writer = new BytesWriter()
      const out = attempt(() => {
        new ParserEventSource(makeCsv(), text).run(program.parserSink(writer, 'json', limits))
        return writer.text()
      })
      if (out.ok) {
        if (undefined === written) written = out.text
        else assert.equal(out.text, written, `max_metadata_bytes ${max}`)
        continue
      }
      assert.equal(out.fail?.code, 'RESOURCE_LIMIT_EXCEEDED', `max_metadata_bytes ${max}: ${out.fail}`)
      assert.equal(out.fail?.limit?.name, 'max_metadata_bytes', `max_metadata_bytes ${max}: ${out.fail}`)
      refused++
    }
    assert.ok(10 < refused, `${refused} bounds refused`)
    assert.ok(undefined !== written, 'no bound let the document through')
  })
})
