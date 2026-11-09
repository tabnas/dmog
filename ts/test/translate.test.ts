/* Copyright (c) 2026 tabnas, MIT License */

// The cross product `dmog translate` answers for (the Rust crate's
// rs/tests/translate_test.rs): every document of the corpus, read with its
// format's grammar, written in every format the command carries, read back
// with that format's grammar, and compared with the document's own value
// under the target's declared conventions (its loss list). A pair that
// fails to write, that writes a document its own grammar refuses, or that
// reads back as anything but the conventions say is a failure, and so is a
// corpus that shrinks.
//
// A pair whose target declares that it refuses the document is held to
// that refusal, its code and the start of its message, and counted: a
// schema-only target (one that writes a schema's tree with no embedding
// into it: C, CSS, PGN, proto and the grammar notations ABNF, EBNF and
// GBNF) refuses another format's tree, a grammar notation refuses a
// grammar spec it has no form for, naming what it met, and Semantic
// Versioning's embedding refuses a tree that is not a version. A refusal of
// another kind, or a document written where a refusal is declared, is a
// failure. The grammar notations share a schema, the grammar spec their
// compilers emit, so each writes the others' documents.
//
// The corpus is the sibling checkouts': transduce's fixtures (aless's: a
// document of every format but C, CSS, expressions, PGN, proto, Semantic
// Versioning and the grammar notations, and more for YAML and ZON), the
// documents of JSONTestSuite every JSON parser must accept, from the copy
// jsonc vendors (`jsonc/test/JSONTestSuite`), as the Go and Rust tests read
// them: json fetches its copy for its own suite, and a checkout of it does
// not hold one; and the example grammars of the grammar notations'
// repositories, which the Rust crate reads in its release run. A fixture
// its own grammar refuses (ZON's repeated fields) is no document, and is
// counted as one refused; so is a document of a format whose reader is a
// registered defect (READER_DEFECTS), counted apart. The Rust crate's
// second matrix, every format's own fixture corpus in release, stays
// Rust's.

import { describe, it } from 'node:test'
import assert from 'node:assert'
import { readFileSync, readdirSync } from 'node:fs'
import { extname, join } from 'node:path'

import { translate as alchemyTranslate } from '@tabnas/alchemy'
import * as expr from '@tabnas/expr'
import { jsonic } from '@tabnas/jsonic'
import { Tabnas } from '@tabnas/parser'
import { BytesWriter } from '@tabnas/render'
import type { Writer } from '@tabnas/render'
import { Datum, Limits, Metrics, toText } from '@tabnas/transduce'

import {
  Compiled,
  Format,
  Request,
  compile,
  format as formatNamed,
  formats,
  names,
  runCompiled,
} from '../dist/translate'

import { sibling } from './host'

const UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

// The format a fixture's extension names, by its id.
function formatOf(extension: string): string | undefined {
  switch (extension) {
    case 'json':
    case 'json5':
    case 'jsonc':
    case 'jsonic':
    case 'jsonl':
    case 'csv':
    case 'toml':
    case 'ini':
    case 'xml':
    case 'yaml':
    case 'zon':
      return extension
    case 'md':
      return 'markdown'
    case 'rss':
    case 'atom':
      return 'feed'
    case 'c':
    case 'h':
      return 'c'
    case 'abnf':
    case 'ebnf':
    case 'gbnf':
      return extension
    default:
      return undefined
  }
}

type Doc = { name: string; id: string; text: string }

// What `test/notation-samples.json` holds: the inputs this repository
// gives the grammar notations' example grammars, the ones their
// repositories' own tests give them, by the name the cross product gives a
// document (`samples`), and the pairs that recognise some of their samples
// otherwise across the lexing, each with exactly those samples, by the name
// the cross product gives a pair (`otherwise`).
const NOTATION: {
  samples: Readonly<Record<string, ReadonlyArray<string>>>
  otherwise: Readonly<Record<string, ReadonlyArray<string>>>
} = JSON.parse(readFileSync(join(__dirname, '..', '..', 'test', 'notation-samples.json'), 'utf8'))

// The grammar notations' example grammars, each notation's directory in
// its repository's checkout.
const NOTATION_EXAMPLES: ReadonlyArray<[string, string]> = [
  ['abnf', sibling('abnf', 'ts', 'test', 'grammar')],
  ['ebnf', sibling('ebnf', 'ts', 'test', 'grammar')],
  ['gbnf', sibling('gbnf', 'test', 'corpus')],
]

// Every document of the corpus: its name, its format and its text. A
// corpus may be held to the files whose names start with a prefix, or to
// those of one format (a grammar notation's examples sit beside a README).
function corpus(): Doc[] {
  const docs: Doc[] = []
  const dirs: Array<[string, string, string | undefined, string | undefined]> = [
    ['transduce', sibling('transduce', 'rs', 'tests', 'fixtures'), undefined, undefined],
    ['JSONTestSuite', sibling('jsonc', 'test', 'JSONTestSuite', 'test_parsing'), 'y_', undefined],
    ...NOTATION_EXAMPLES.map(([id, dir]): [string, string, undefined, string] => [id, dir, undefined, id]),
  ]
  for (const [name, dir, prefix, only] of dirs) {
    for (const file of readdirSync(dir).sort()) {
      if (undefined !== prefix && !file.startsWith(prefix)) continue
      const id = formatOf(extname(file).slice(1))
      if (undefined === id || (undefined !== only && only !== id)) continue
      let text: string
      try {
        text = UTF8.decode(readFileSync(join(dir, file)))
      } catch (_err) {
        continue
      }
      docs.push({ name: `${name}/${file}`, id, text })
    }
  }
  return docs
}

function format(id: string): Format {
  const found = formatNamed(id)
  assert.ok(found, `${id} is a format`)
  return found
}

// The compositions compiled so far, by source, target and program: one
// compiled composition serves every document of a pair.
const COMPILED = new Map<string, Compiled>()

// Translate `text`, read as `from`, into `to`, with `program` in front
// when there is one; read whole, as `--whole` reads it, where `whole` is
// asked.
function translateText(
  from: Format,
  to: Format,
  text: string,
  program?: { file: string; text: string },
  whole = false,
): string {
  const request: Request = {
    from,
    to,
    options: alchemyTranslate.Options.default(),
    program,
    whole,
    limits: Limits.default(),
  }
  const key = JSON.stringify([from.id, to.id, program?.text ?? null, whole])
  let compiled = COMPILED.get(key)
  if (undefined === compiled) {
    compiled = compile(request)
    COMPILED.set(key, compiled)
  }
  const out = new BytesWriter()
  runCompiled(request, compiled, text, out, new Metrics())
  // A translation writes UTF-8.
  return UTF8.decode(out.bytes())
}

// What a failure says, for a report: a `Fail`'s code, message and
// position, or what else was thrown.
function why(err: unknown): string {
  return String(err)
}

// Whether a translation failed because the source's transducer part
// cannot stream the document (a repeated member, a stream of several YAML
// documents, a merge key): its code, and the route that reads it named,
// since the output streams and is never written twice (admin ADR-33).
function unstreamed(err: unknown): boolean {
  const fail = err as { code?: string; message?: string } | undefined
  return (
    ('STREAMABILITY_UNKNOWN' === fail?.code || 'DUPLICATE_MEMBER' === fail?.code) &&
    String(fail?.message).endsWith('; --whole reads the document whole')
  )
}

// ---------------------------------------------------------------------
// The readers registered as defective
// ---------------------------------------------------------------------

// The formats whose reader, as this command reads a document through
// transduce, does not build the tree the format's parts declare, for a
// defect of the format's package: each id with its defect (the Rust
// crate's READER_DEFECTS, for this runtime's readers). The matrix reads no
// document of a registered format as a source, and counts the documents it
// leaves out; every format is still a target. 'a registered reader defect
// still stands' holds each entry to an example document, so an entry fails
// once its reader is repaired, and must then be deleted.
const READER_DEFECTS: ReadonlyArray<[string, string]> = [
  [
    'semver',
    "@tabnas/semver's reader builds a number past 2^53 - 1 as a bigint, which transduce rounds to a " +
      'double, so 99999999999999999999.1.2 is written as 100000000000000000000.1.2; the Rust reader ' +
      "keeps the digits, as the format's render takes them (its part's alchemy/render.alc). The format is " +
      'left out whole while it is registered',
  ],
]

// Whether documents of `id` are left out as sources for a registered reader
// defect.
function registeredDefect(id: string): boolean {
  return READER_DEFECTS.some(([defective]) => defective === id)
}

// ---------------------------------------------------------------------
// The refusals the targets declare
// ---------------------------------------------------------------------

// What a pair is held to: written, and read back under the target's
// conventions; refused as the target declares, with the code and the start
// of the message alchemy's composition or the target's part gives; or
// written unless the target refuses it so (`unless`), where the target
// declares that it refuses what it has no form for, naming what it met.
type Expect =
  | { written: true; unless?: { code: string; reason: string } }
  | { written: false; code: string; reason: string }

// Whether a format is a grammar notation: one whose documents read as the
// grammar spec the tabnas BNF compiler emits, which each notation's render
// writes back.
function grammarNotation(format: Format): boolean {
  return 'grammar-spec' === format.part.schema
}

// What `from`'s document, read as `source`, into `to` is held to. A
// schema-only target (one that writes from a tree, with a schema and no
// embed: C, CSS, PGN, proto and the grammar notations) refuses a tree of
// another schema before any output, as alchemy's composition declares; a
// grammar notation's render writes a grammar spec, any notation's, unless
// it has no form for something in it, which it refuses naming what it met,
// as its loss list declares (an action, a negated class in ABNF, the
// engine's own tokens in GBNF, ...); Semantic Versioning's embedding
// refuses a tree that is not a version, as its part declares; and expr's
// render refuses a list led by an object whose `src` its reader would take
// for the list's operator, where that is no default operator's source text
// (a C syntax tree's token), as its loss list declares (exprRefuses).
// Every other pair is written, Markdown's table among them: it writes from
// records, which any tree makes.
function expect(from: Format, to: Format, source: Datum): Expect {
  const target = to.part
  const foreign = 'tree' === target.writes && undefined !== target.schema && from.part.schema !== target.schema
  if (foreign && undefined === target.embed) {
    return {
      written: false,
      code: 'TARGET_VALUE_UNREPRESENTABLE',
      reason: `schema_only: ${target.id} writes a ${target.schema} tree, `,
    }
  }
  if (foreign && 'semver' === to.id && !semverVersion(source)) {
    return { written: false, code: 'TARGET_VALUE_UNREPRESENTABLE', reason: 'the document is not a version: ' }
  }
  if ('expr' === to.id && exprRefuses(source)) {
    return { written: false, code: 'TARGET_VALUE_UNREPRESENTABLE', reason: 'the object at ' }
  }
  if (grammarNotation(to)) {
    return {
      written: true,
      unless: {
        code: 'TARGET_VALUE_UNREPRESENTABLE',
        reason: `the grammar spec cannot be written as ${to.id.toUpperCase()}: `,
      },
    }
  }
  return { written: true }
}

// The text a number is written with when it is digits alone: its lexeme,
// or the text JSON writes for it, ECMAScript's, which spells a whole number
// below 10^21 with its digits.
function writtenDigits(d: Datum): string | undefined {
  if ('number' !== d.type) return undefined
  let text: string
  if (null !== d.lexeme) text = d.lexeme
  else if (Number.isFinite(d.value) && Number.isInteger(d.value) && !Object.is(d.value, -0) && 0 <= d.value && d.value < 1e21)
    text = BigInt(d.value).toString()
  else return undefined
  return /^[0-9]+$/.test(text) ? text : undefined
}

// Whether a string is digits with no leading zero, but for `0` itself.
function digitString(s: string): boolean {
  return /^[0-9]+$/.test(s) && ('0' === s || !s.startsWith('0'))
}

// Whether a value is a prerelease (`prerelease`) or build identifier: a
// number written in digits, or a string of 0-9, A-Z, a-z and -, not empty,
// which for a prerelease is no number of digits that begins with a zero.
function semverIdentifier(d: Datum, prerelease: boolean): boolean {
  if ('number' === d.type) return undefined !== writtenDigits(d)
  if ('string' !== d.type) return false
  const s = d.value
  return /^[0-9A-Za-z-]+$/.test(s) && !(prerelease && /^[0-9]+$/.test(s) && !digitString(s))
}

// Whether a value is a version as Semantic Versioning's embedding takes one
// (its part's alchemy/embed.alc): an object whose major, minor and patch
// are whole numbers written in digits (a number whose text is digits alone,
// or a string of digits with no leading zero), and whose prerelease and
// build, where it has them, are null, the empty string, or a list of
// identifiers or one string of them joined by dots.
function semverVersion(d: Datum): boolean {
  if ('object' !== d.type) return false
  const m = d.members
  const core = (key: string): boolean => {
    const v = m.get(key)
    if (undefined === v) return false
    if ('number' === v.type) return undefined !== writtenDigits(v)
    return 'string' === v.type && digitString(v.value)
  }
  const identifiers = (key: string, prerelease: boolean): boolean => {
    const v = m.get(key)
    if (undefined === v || 'null' === v.type) return true
    if ('string' === v.type) {
      return '' === v.value || v.value.split('.').every((id) => semverIdentifier({ type: 'string', value: id }, prerelease))
    }
    if ('array' === v.type) return v.items.every((i) => semverIdentifier(i, prerelease))
    return false
  }
  return core('major') && core('minor') && core('patch') && identifiers('prerelease', true) && identifiers('build', false)
}

// ---------------------------------------------------------------------
// Values compared as the conventions compare them
// ---------------------------------------------------------------------

const NULL: Datum = { type: 'null' }

function arr(items: Datum[]): Datum {
  return { type: 'array', items }
}

function obj(members: Iterable<[string, Datum]>): Datum {
  return { type: 'object', members: new Map(members) }
}

// Two values the same: numbers by value (NaN is NaN), objects by their
// members whatever their order, arrays in order.
function same(a: Datum, b: Datum): boolean {
  if ('number' === a.type && 'number' === b.type) {
    return a.value === b.value || (Number.isNaN(a.value) && Number.isNaN(b.value))
  }
  if ('array' === a.type && 'array' === b.type) {
    return a.items.length === b.items.length && a.items.every((x, i) => same(x, b.items[i]))
  }
  if ('object' === a.type && 'object' === b.type) {
    if (a.members.size !== b.members.size) return false
    for (const [k, v] of a.members) {
      const w = b.members.get(k)
      if (undefined === w || !same(v, w)) return false
    }
    return true
  }
  switch (a.type) {
    case 'null':
      return 'null' === b.type
    case 'bool':
      return 'bool' === b.type && a.value === b.value
    case 'string':
      return 'string' === b.type && a.value === b.value
    default:
      return false
  }
}

// The value with every number that is not finite replaced.
function mapNonFinite(d: Datum, f: (n: number) => Datum): Datum {
  switch (d.type) {
    case 'number':
      return Number.isFinite(d.value) ? d : f(d.value)
    case 'array':
      return arr(d.items.map((i) => mapNonFinite(i, f)))
    case 'object':
      return obj([...d.members].map(([k, v]): [string, Datum] => [k, mapNonFinite(v, f)]))
    default:
      return d
  }
}

function wrapObject(d: Datum, key: string): Datum {
  return 'object' === d.type ? d : obj([[key, d]])
}

function wrapArray(d: Datum): Datum {
  return 'array' === d.type ? d : arr([d])
}

// TOML's conventions: no null (a member whose value is null is not
// written, a null element is skipped).
function withoutNulls(d: Datum): Datum {
  switch (d.type) {
    case 'array':
      return arr(d.items.filter((i) => 'null' !== i.type).map(withoutNulls))
    case 'object':
      return obj(
        [...d.members].filter(([, v]) => 'null' !== v.type).map(([k, v]): [string, Datum] => [k, withoutNulls(v)]),
      )
    default:
      return d
  }
}

// A string as Rust's `str::parse::<f64>` reads it: an optional sign, then
// `inf`, `infinity` or `nan` in any case, or digits with an optional point
// and exponent; nothing else, no blanks.
function rustF64(text: string): number | undefined {
  const m = /^([+-]?)(?:(inf|infinity|nan)|((?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?))$/i.exec(text)
  if (null === m) return undefined
  if (undefined !== m[2]) {
    const word = m[2].toLowerCase()
    if ('nan' === word) return Number.NaN
    return '-' === m[1] ? Number.NEGATIVE_INFINITY : Number.POSITIVE_INFINITY
  }
  return Number(m[1] + m[3])
}

// Rust's `char::is_whitespace`: Unicode's White_Space property.
function rustSpace(c: number): boolean {
  return (
    (0x09 <= c && c <= 0x0d) ||
    0x20 === c ||
    0x85 === c ||
    0xa0 === c ||
    0x1680 === c ||
    (0x2000 <= c && c <= 0x200a) ||
    0x2028 === c ||
    0x2029 === c ||
    0x202f === c ||
    0x205f === c ||
    0x3000 === c
  )
}

// Rust's `str::trim`: Unicode's whitespace at either end.
function rustTrim(text: string): string {
  let start = 0
  let end = text.length
  while (start < end && rustSpace(text.charCodeAt(start))) start++
  while (end > start && rustSpace(text.charCodeAt(end - 1))) end--
  return text.slice(start, end)
}

// The text a cell is written as in CSV and Markdown: a string as it is, a
// number by its value (compared as one), a non-finite one by its word, a
// boolean by its name, null and an absent member as the empty field, a
// container as its compact JSON text.
type Cell = { kind: 'text'; text: string } | { kind: 'number'; value: number }

function textCell(text: string): Cell {
  return { kind: 'text', text }
}

function cell(d: Datum | undefined): Cell {
  if (undefined === d || 'null' === d.type) return textCell('')
  switch (d.type) {
    case 'bool':
      return textCell(String(d.value))
    case 'number':
      if (Number.isNaN(d.value)) return textCell('NaN')
      if (!Number.isFinite(d.value)) return textCell(d.value > 0 ? 'Infinity' : '-Infinity')
      return { kind: 'number', value: d.value }
    case 'string':
      return textCell(d.value)
    default:
      return textCell(toText(d))
  }
}

function cellIs(expected: Cell, got: string): boolean {
  if ('text' === expected.kind) return expected.text === got
  const n = rustF64(got)
  return undefined !== n && n === expected.value
}

function cellText(c: Cell): string {
  return 'text' === c.kind ? JSON.stringify(c.text) : String(c.value)
}

// The table the inferred binding makes of a value: the rows are the root
// array's elements (a root of another kind is one row), the columns the
// first row's (an object's keys, an array's positions, or one `value`
// column for a scalar), each row's cell found by the column's path.
function table(d: Datum): { labels: string[]; rows: Cell[][] } {
  const root = wrapArray(d)
  const rows = 'array' === root.type ? root.items : []
  type Path = { key: string } | { index: number } | { itself: true }
  let columns: Array<[string, Path]> = []
  const first = rows[0]
  if (undefined === first) columns = []
  else if ('object' === first.type) columns = [...first.members.keys()].map((k): [string, Path] => [k, { key: k }])
  else if ('array' === first.type) columns = first.items.map((_, i): [string, Path] => [String(i), { index: i }])
  else columns = [['value', { itself: true }]]
  const cells = rows.map((row) =>
    columns.map(([, path]) => {
      if ('key' in path) return cell('object' === row.type ? row.members.get(path.key) : undefined)
      if ('index' in path) return cell('array' === row.type ? row.items[path.index] : undefined)
      return cell(row)
    }),
  )
  return { labels: columns.map(([label]) => label), rows: cells }
}

// Whether a read-back table (an array of objects keyed by label, every
// value a string) is the table the inferred binding makes of `source`,
// with each cell's text passed through `normal` first (Markdown's
// normalisation of what it writes). Undefined when it is; why not when it
// is not.
function checkRecords(source: Datum, back: Datum, normal: (text: string) => string): string | undefined {
  const { labels, rows } = table(source)
  if ('array' !== back.type) return `read back as ${toText(back)}, not an array of records`
  const backRows = back.items
  if (0 === labels.length) {
    return 0 === backRows.length ? undefined : `a table of no columns read back as ${toText(back)}`
  }
  if (backRows.length !== rows.length) {
    return `${backRows.length} rows read back, ${rows.length} written: ${toText(back)}`
  }
  for (let i = 0; i < rows.length; i++) {
    const got = backRows[i]
    if ('object' !== got.type) return `row ${i} read back as ${toText(got)}`
    for (let c = 0; c < labels.length; c++) {
      const label = labels[c]
      // A label is a header cell, written and read back as any cell.
      const found = got.members.get(normal(label))
      const text =
        undefined === found || 'null' === found.type ? '' : 'string' === found.type ? found.value : toText(found)
      const written = rows[i][c]
      const expected: Cell = 'text' === written.kind ? textCell(normal(written.text)) : written
      if (!cellIs(expected, text)) {
        return (
          `row ${i}, column ${JSON.stringify(label)}: read back ${JSON.stringify(text)}, ` +
          `where ${cellText(expected)} was written`
        )
      }
    }
  }
  return undefined
}

// Markdown's normalisation of a written cell: a line break is a space, a
// U+0000 is U+FFFD, and the whitespace at either end is not kept, as the
// reader trims it: what JavaScript's `trim` takes, which is Unicode's
// whitespace without U+0085 and with U+FEFF.
function markdownCell(text: string): string {
  return text.replace(/\r\n/g, ' ').replace(/[\n\r]/g, ' ').replace(/\0/g, '\ufffd').trim()
}

// ---------------------------------------------------------------------
// The cross product
// ---------------------------------------------------------------------

// Whether an INI document read back (`back`) is what INI's conventions
// make of `expected`: an object is a section (or the root) and an array of
// scalars is `key[]` lines, each read back as itself; a number reads back
// as its text, and one that is not finite as its word; a container INI has
// no place for (inside an array, an empty array, or under a key no header
// can spell) reads back as its compact JSON text, a string; true, false
// and null read back as themselves, and a string as itself.
function iniSame(expected: Datum, back: Datum): boolean {
  if ('object' === expected.type && 'object' === back.type) {
    if (expected.members.size !== back.members.size) return false
    for (const [k, v] of expected.members) {
      const w = back.members.get(k) ?? back.members.get(rustTrim(k))
      if (undefined === w || !iniSame(v, w)) return false
    }
    return true
  }
  if ('array' === expected.type && 'array' === back.type && 0 < expected.items.length) {
    return (
      expected.items.length === back.items.length && expected.items.every((x, i) => iniItem(x, back.items[i]))
    )
  }
  if ('number' === expected.type && 'string' === back.type) return numberTextIs(expected.value, back.value)
  if (('object' === expected.type || 'array' === expected.type) && 'string' === back.type) {
    return jsonTextIs(expected, back.value)
  }
  return same(expected, back)
}

// An array item: a scalar as `iniSame` reads it, a container as its JSON
// text.
function iniItem(expected: Datum, back: Datum): boolean {
  if (('object' === expected.type || 'array' === expected.type) && 'string' === back.type) {
    return jsonTextIs(expected, back.value)
  }
  return iniSame(expected, back)
}

// Whether `text` spells the number `value`: its digits, or the word of one
// that is not finite.
function numberTextIs(value: number, text: string): boolean {
  switch (text) {
    case 'Infinity':
      return Number.POSITIVE_INFINITY === value
    case '-Infinity':
      return Number.NEGATIVE_INFINITY === value
    case 'NaN':
      return Number.isNaN(value)
    default: {
      const n = rustF64(text)
      return undefined !== n && n === value
    }
  }
}

function own(o: object, k: string): unknown {
  return Object.prototype.hasOwnProperty.call(o, k) ? (o as Record<string, unknown>)[k] : undefined
}

// Whether `text` is the compact JSON text of `container`, read back as
// JSON and compared as values, a number that is not finite matching null
// or its word.
function jsonTextIs(container: Datum, text: string): boolean {
  let json: unknown
  try {
    json = JSON.parse(text)
  } catch (_err) {
    return false
  }
  const matches = (d: Datum, j: unknown): boolean => {
    if (null === j) return 'null' === d.type || 'number' === d.type
    switch (d.type) {
      case 'bool':
        return 'boolean' === typeof j && d.value === j
      case 'number':
        if ('number' === typeof j) return j === d.value
        return 'string' === typeof j && numberTextIs(d.value, j)
      case 'string':
        return 'string' === typeof j && d.value === j
      case 'array':
        return Array.isArray(j) && d.items.length === j.length && d.items.every((x, i) => matches(x, j[i]))
      case 'object': {
        if ('object' !== typeof j || Array.isArray(j)) return false
        if (d.members.size !== Object.keys(j as object).length) return false
        for (const [k, v] of d.members) {
          if (!Object.prototype.hasOwnProperty.call(j, k) || !matches(v, own(j as object, k))) return false
        }
        return true
      }
      default:
        return false
    }
  }
  return matches(container, json)
}

// Whether a string spells an integer as ZON's reader writes a big
// integer's digits, which is when the render writes a lone `$big` as the
// integer itself: a minus sign at most, and first, then `0` or digits that
// do not begin with `0`, but not `-0`.
function zonBigDigits(s: string): boolean {
  const digits = s.startsWith('-') ? s.slice(1) : s
  return 0 < digits.length && /^[0-9]+$/.test(digits) && ('0' === digits || !digits.startsWith('0')) && '-0' !== s
}

// The integer an object whose only member is `$big` spells, when its
// value is a big integer's digits: ZON's render writes that object as the
// integer, and its reader builds an integer no double holds exactly as
// that object.
function zonBig(d: Datum): Datum | undefined {
  if ('object' !== d.type || 1 !== d.members.size) return undefined
  const big = d.members.get('$big')
  if (undefined === big || 'string' !== big.type || !zonBigDigits(big.value)) return undefined
  return { type: 'number', value: Number(big.value), lexeme: big.value }
}

// What ZON's conventions make of a value it is given: an empty struct
// reads back as an empty tuple, and a lone `$big` holding a big integer's
// digits as that integer.
function zonReading(d: Datum): Datum {
  if ('object' === d.type && 0 === d.members.size) return arr([])
  const big = zonBig(d)
  if (undefined !== big) return big
  if ('array' === d.type) return arr(d.items.map(zonReading))
  if ('object' === d.type) return obj([...d.members].map(([k, v]): [string, Datum] => [k, zonReading(v)]))
  return d
}

// A field name as ZON's render wrote it, read back by the declared reverse
// of its convention: `$empty` is the empty name; `$$` and a rest is `$`
// and the rest; `$json:` and a text is the string the text spells as a
// double-quoted JSON string; any other name is as it is.
function zonName(written: string): string {
  if ('$empty' === written) return ''
  if (written.startsWith('$$')) return '$' + written.slice(2)
  if (written.startsWith('$json:')) {
    try {
      const name = JSON.parse(written.slice('$json:'.length))
      return 'string' === typeof name ? name : written
    } catch (_err) {
      return written
    }
  }
  return written
}

// What ZON's reader made of a document its render wrote, as the value it
// was: a lone `$big` (the reader's big integer) is the integer, and every
// field name reads back by the reverse of the convention that wrote it.
function zonBack(d: Datum): Datum {
  const big = zonBig(d)
  if (undefined !== big) return big
  if ('array' === d.type) return arr(d.items.map(zonBack))
  if ('object' === d.type) return obj([...d.members].map(([k, v]): [string, Datum] => [zonName(k), zonBack(v)]))
  return d
}

// The default operators' source texts, which expr's render writes as the
// operator (`(` a group): an operation is a list whose first element is one,
// or an object whose `src` is one.
const EXPR_OPERATORS: ReadonlyArray<string> = ['+', '-', '*', '/', '%', '(']

// Whether expr's render refuses a tree (its loss list): it holds a list
// whose first element is an object with a `src` member a simplifier would
// read as the list's operator, anything but the empty string, false, null
// or zero, which is no default operator's source text, as the tokens a C
// syntax tree puts first in a node's children are.
function exprRefuses(d: Datum): boolean {
  if ('array' === d.type) {
    const first = d.items[0]
    if (undefined !== first && 'object' === first.type) {
      const src = first.members.get('src')
      const read =
        undefined === src || 'null' === src.type
          ? false
          : 'bool' === src.type
            ? src.value
            : 'string' === src.type
              ? '' !== src.value && !EXPR_OPERATORS.includes(src.value)
              : 'number' === src.type
                ? 0 !== src.value || (null !== src.lexeme && src.lexeme.startsWith('-'))
                : true
      if (read) return true
    }
    return d.items.some(exprRefuses)
  }
  if ('object' === d.type) return [...d.members.values()].some(exprRefuses)
  return false
}

// A tree as expr's reader reads one back (its simplified tree, the form its
// shared fixtures hold): a list whose first element is an object whose
// `src` is a default operator's source text has that text in the object's
// place, as expr's loss list declares. Any other such `src` its render
// refuses (exprRefuses), so no tree holding one is read back.
function exprSimplify(d: Datum): Datum {
  if ('array' === d.type) {
    return arr(
      d.items.map((item, i) => {
        if (0 === i && 'object' === item.type) {
          const src = item.members.get('src')
          if (undefined !== src && 'string' === src.type && EXPR_OPERATORS.includes(src.value)) return src
        }
        return exprSimplify(item)
      }),
    )
  }
  if ('object' === d.type) return obj([...d.members].map(([k, v]): [string, Datum] => [k, exprSimplify(v)]))
  return d
}

// What expr's conventions make of a tree it is given (its loss list): an
// operator's description reads back as its source text; a negative number,
// written with its sign, as the operator `-` applied to its magnitude; and a
// number that is not finite as the string Infinity, -Infinity or NaN.
function exprReading(d: Datum): Datum {
  const numbers = (d: Datum): Datum => {
    if ('number' === d.type) {
      if (Number.isNaN(d.value)) return { type: 'string', value: 'NaN' }
      if (!Number.isFinite(d.value)) return { type: 'string', value: 0 < d.value ? 'Infinity' : '-Infinity' }
      // JavaScript writes -0 with no sign; a lexeme keeps the one it had.
      const signed = null !== d.lexeme ? d.lexeme.startsWith('-') : d.value < 0
      if (signed) return arr([{ type: 'string', value: '-' }, { type: 'number', value: Math.abs(d.value), lexeme: null }])
      return d
    }
    if ('array' === d.type) return arr(d.items.map(numbers))
    if ('object' === d.type) return obj([...d.members].map(([k, v]): [string, Datum] => [k, numbers(v)]))
    return d
  }
  return numbers(exprSimplify(d))
}

// The largest integer every runtime's reader of a version keeps as a
// number, 2^53 - 1; Semantic Versioning's Rust reader keeps one past it as
// its digits.
const MAX_SAFE_INTEGER = Number.MAX_SAFE_INTEGER

// A version's number as Semantic Versioning's reader builds it from its
// digits: a number up to 2^53 - 1, and its digits past it.
function semverNumber(digits: string): Datum {
  const value = Number(digits)
  return value <= MAX_SAFE_INTEGER ? { type: 'number', value, lexeme: null } : { type: 'string', value: digits }
}

// What a version reads back as, by Semantic Versioning's loss list: its
// major, minor and patch, each the number its digits make; its prerelease
// and build as lists, empty where they are absent, null or empty, one string
// split at its dots; a prerelease identifier of digits as the number they
// make and a build identifier as its text; and no other member.
function semverReading(d: Datum): Datum {
  if ('object' !== d.type) throw new Error('a version is an object')
  const m = d.members
  const text = (v: Datum): string => {
    if ('string' === v.type) return v.value
    const digits = writtenDigits(v)
    if (undefined === digits) throw new Error("a version's numbers are written in digits")
    return digits
  }
  const identifiers = (key: string, prerelease: boolean): Datum => {
    const v = m.get(key)
    const items: Datum[] =
      undefined !== v && 'string' === v.type && '' !== v.value
        ? v.value.split('.').map((id): Datum => ({ type: 'string', value: id }))
        : undefined !== v && 'array' === v.type
          ? v.items
          : []
    return arr(
      items.map((item): Datum => {
        const id = text(item)
        return prerelease && /^[0-9]+$/.test(id) ? semverNumber(id) : { type: 'string', value: id }
      }),
    )
  }
  const core = (key: string): Datum => {
    const v = m.get(key)
    if (undefined === v) throw new Error(`a version has a ${key}`)
    return semverNumber(text(v))
  }
  return obj([
    ['major', core('major')],
    ['minor', core('minor')],
    ['patch', core('patch')],
    ['prerelease', identifiers('prerelease', true)],
    ['build', identifiers('build', false)],
  ])
}

// The id, and an author's uri, the feed render supplies where a feed has
// none.
const FEED_RENDER = 'tag:tabnas.dev,2026:feed-render'

// The date the feed render supplies where a feed or an entry has none, the
// one its embedding gives a plain tree.
const FEED_EPOCH = '1970-01-01T00:00:00Z'

// Whether `s` is `n` decimal digits.
function digitsOf(n: number, s: string): boolean {
  return s.length === n && /^[0-9]*$/.test(s)
}

// Whether `s` is two digits from `lo` to `hi`.
function twoDigits(lo: number, hi: number, s: string): boolean {
  return digitsOf(2, s) && lo <= Number(s) && Number(s) <= hi
}

// Whether `s` is an RFC 3339 date-time with the upper-case T and Z Atom asks
// for, as the feed render reads one: a full date; a time, whose seconds may
// have a fraction; and Z or an offset.
function rfc3339(s: string): boolean {
  const second = (sec: string): boolean => {
    const p = sec.split('.')
    if (1 === p.length) return twoDigits(0, 60, sec)
    return 2 === p.length && twoDigits(0, 60, p[0]) && /^[0-9]+$/.test(p[1])
  }
  const time = (t: string): boolean => {
    const p = t.split(':')
    return 3 === p.length && twoDigits(0, 23, p[0]) && twoDigits(0, 59, p[1]) && second(p[2])
  }
  const offset = (o: string): boolean => {
    const p = o.split(':')
    return 2 === p.length && twoDigits(0, 23, p[0]) && twoDigits(0, 59, p[1])
  }
  const parts = s.split('T')
  if (2 !== parts.length) return false
  const [date, rest] = parts
  const d = date.split('-')
  if (!(3 === d.length && digitsOf(4, d[0]) && twoDigits(1, 12, d[1]) && twoDigits(1, 31, d[2]))) return false
  const z = rest.split('Z')
  if (2 === z.length) return '' === z[1] && time(z[0])
  if (1 !== z.length) return false
  const plus = rest.split('+')
  if (2 === plus.length) return time(plus[0]) && offset(plus[1])
  if (1 !== plus.length) return false
  const minus = rest.split('-')
  return 2 === minus.length && time(minus[0]) && offset(minus[1])
}

// An RSS date, RFC 822's date-time with a four-digit year allowed and its
// names in any case, as the same instant in RFC 3339's form, as the feed
// render writes one: a day of the week and a comma at most, then the day,
// the month, the year (two digits before 50 in the 2000s, else in the
// 1900s), the time (seconds 00 where it has none) and the zone (Z for UT,
// GMT and Z, the US zones' offsets, or a sign and four digits). Its tabs and
// line breaks are spaces.
function rfc822(s: string): string | undefined {
  const words = (t: string): string[] =>
    t
      .replace(/[\n\r\t]/g, ' ')
      .split(' ')
      .filter((w) => '' !== w)
  const parts = s.split(',')
  let w: string[]
  if (1 === parts.length) w = words(s)
  else if (2 === parts.length) {
    const d = words(parts[0])
    if (!(1 === d.length && ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'].includes(asciiLower(d[0])))) {
      return undefined
    }
    w = words(parts[1])
  } else return undefined
  if (5 !== w.length) return undefined
  const [day0, month0, year0, time0, zone] = w
  let year: string
  if (digitsOf(4, year0)) year = year0
  else if (digitsOf(2, year0)) year = (year0 < '50' ? '20' : '19') + year0
  else return undefined
  const m = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'].indexOf(
    asciiLower(month0),
  )
  if (m < 0) return undefined
  const month = String(m + 1).padStart(2, '0')
  const day = digitsOf(1, day0) ? `0${day0}` : day0
  if (!twoDigits(1, 31, day)) return undefined
  const hms = time0.split(':')
  if (2 === hms.length) hms.push('00')
  if (!(3 === hms.length && twoDigits(0, 23, hms[0]) && twoDigits(0, 59, hms[1]) && twoDigits(0, 60, hms[2]))) {
    return undefined
  }
  const numeric = (sign: string, digits: string): string | undefined =>
    digitsOf(4, digits) && twoDigits(0, 23, digits.slice(0, 2)) && twoDigits(0, 59, digits.slice(2))
      ? `${sign}${digits.slice(0, 2)}:${digits.slice(2)}`
      : undefined
  const zones: Record<string, string> = {
    ut: 'Z',
    gmt: 'Z',
    z: 'Z',
    est: '-05:00',
    edt: '-04:00',
    cst: '-06:00',
    cdt: '-05:00',
    mst: '-07:00',
    mdt: '-06:00',
    pst: '-08:00',
    pdt: '-07:00',
  }
  // A sign and four digits; the render splits the zone at its signs, so one
  // with a second sign is no zone.
  const plus = zone.split('+')
  const minus = zone.split('-')
  const offset = Object.prototype.hasOwnProperty.call(zones, asciiLower(zone))
    ? zones[asciiLower(zone)]
    : 2 === plus.length && '' === plus[0]
      ? numeric('+', plus[1])
      : 2 === minus.length && '' === minus[0]
        ? numeric('-', minus[1])
        : undefined
  if (undefined === offset) return undefined
  return `${year}-${month}-${day}T${hms.join(':')}${offset}`
}

// A string with its ASCII letters in lower case, as RFC 822's names may be
// in any case.
function asciiLower(s: string): string {
  return s.replace(/[A-Z]/g, (c) => c.toLowerCase())
}

// A date as the feed render writes it: its RFC 3339 form, or the text of
// one in neither form (`kept`), which it writes as the epoch with a
// category that keeps the text. A date with no text is a missing one.
function feedDate(v: Datum): { date: string } | { kept: string } {
  if ('string' !== v.type) return { kept: toText(v) }
  if ('' === v.value) return { date: FEED_EPOCH }
  const upper = v.value.replace(/t/g, 'T').replace(/z/g, 'Z')
  if (rfc3339(upper)) return { date: upper }
  const c = rfc822(v.value)
  return undefined === c ? { kept: v.value } : { date: c }
}

// A feed's value as the reader builds it back: no member whose value is
// null, and a character XML 1.0 cannot carry as U+FFFD.
function feedClean(d: Datum): Datum {
  if ('string' === d.type) {
    // eslint-disable-next-line no-control-regex
    return { type: 'string', value: d.value.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\ufffe\uffff]/g, '\ufffd') }
  }
  if ('array' === d.type) return arr(d.items.map(feedClean))
  if ('object' === d.type) {
    return obj([...d.members].filter(([, v]) => 'null' !== v.type).map(([k, v]): [string, Datum] => [k, feedClean(v)]))
  }
  return d
}

// A text construct or a content as the feed render writes it: one of type
// xhtml as html, its value trimmed.
function feedText(d: Datum): Datum {
  const cleaned = feedClean(d)
  if ('object' !== cleaned.type) return cleaned
  const type = cleaned.members.get('type')
  if (undefined === type || 'string' !== type.type || 'xhtml' !== type.value) return cleaned
  return obj(
    [...cleaned.members].map(([k, v]): [string, Datum] => {
      if ('type' === k) return [k, { type: 'string', value: 'html' }]
      if ('value' === k && 'string' === v.type) return [k, { type: 'string', value: v.value.trim() }]
      return [k, v]
    }),
  )
}

// Whether a feed or an entry has an author: a list of them that is not
// empty.
function feedHasAuthor(d: Datum): boolean {
  if ('object' !== d.type) return false
  const authors = d.members.get('authors')
  return undefined !== authors && 'array' === authors.type && 0 < authors.items.length
}

// A feed's or an entry's members as the feed render writes them and the
// reader builds them back (`feedReading`): each date in RFC 3339's form, one
// the render cannot read as the epoch with a category keeping its text,
// which comes before the object's own categories where the date comes
// before them in the tree's order; text constructs as `feedText`; an id, a
// title and an updated where the object has none, an entry's id with a
// slash and `position`; an entry's source not read back.
function feedObject(d: Datum, position: number | undefined): Array<[string, Datum]> {
  if ('object' !== d.type) throw new Error('a feed and an entry are objects')
  const out: Array<[string, Datum]> = []
  const before: Datum[] = []
  const after: Datum[] = []
  let categoriesMet = false
  for (const [k, v] of d.members) {
    if ('null' === v.type) continue
    if ('categories' === k) categoriesMet = true
    if ('source' === k || 'format' === k || 'version' === k || 'entries' === k) continue
    let value: Datum
    if ('updated' === k || 'published' === k) {
      const date = feedDate(v)
      if ('date' in date) value = { type: 'string', value: date.date }
      else {
        const category = obj([
          ['term', { type: 'string', value: k }],
          ['scheme', { type: 'string', value: `${FEED_RENDER}/date` }],
          ['label', feedClean({ type: 'string', value: date.kept })],
        ])
        if (categoriesMet) after.push(category)
        else before.push(category)
        value = { type: 'string', value: FEED_EPOCH }
      }
    } else if (['title', 'subtitle', 'rights', 'summary', 'content'].includes(k)) value = feedText(v)
    else value = feedClean(v)
    out.push([k, value])
  }
  if (0 < before.length || 0 < after.length) {
    const at = out.findIndex(([k]) => 'categories' === k)
    let own: Datum[] = []
    if (0 <= at) {
      const [, categories] = out.splice(at, 1)[0]
      if ('array' === categories.type) own = categories.items
    }
    out.push(['categories', arr([...before, ...own, ...after])])
  }
  const has = (key: string): boolean => out.some(([k]) => key === k)
  if (!has('id')) out.push(['id', { type: 'string', value: undefined === position ? FEED_RENDER : `${FEED_RENDER}/${position}` }])
  if (!has('title')) {
    out.push([
      'title',
      obj([
        ['type', { type: 'string', value: 'text' }],
        ['value', { type: 'string', value: '' }],
      ]),
    ])
  }
  if (!has('updated')) out.push(['updated', { type: 'string', value: FEED_EPOCH }])
  return out
}

// What a feed reads back as, by its loss list: an Atom 1.0 feed of its
// members and its entries as `feedObject` writes them, and an author, named
// by the feed's title where that is text and not empty and `unknown`
// otherwise, where the feed has none, though every entry may have one: the
// render writes a feed's elements before its entries, each entry as it
// comes, so it cannot see them first (@tabnas/feed 0.6.19).
function feedReading(d: Datum): Datum {
  if ('object' !== d.type) throw new Error('a feed is an object')
  const members = d.members
  const given = members.get('entries')
  const entries =
    undefined !== given && 'array' === given.type ? given.items.map((e, i) => obj(feedObject(e, i))) : []
  const out: Array<[string, Datum]> = [
    ['format', { type: 'string', value: 'atom' }],
    ['version', { type: 'string', value: '1.0' }],
  ]
  if (members.has('entries')) out.push(['entries', arr(entries)])
  out.push(...feedObject(d, undefined))
  if (!feedHasAuthor(d)) {
    const title = members.get('title')
    let name: Datum = { type: 'string', value: 'unknown' }
    if (undefined !== title && 'object' === title.type) {
      const type = title.members.get('type')
      const value = title.members.get('value')
      if (
        undefined !== type &&
        'string' === type.type &&
        'text' === type.value &&
        undefined !== value &&
        'string' === value.type &&
        '' !== value.value
      ) {
        name = value
      }
    }
    const author = obj([
      ['name', name],
      ['uri', { type: 'string', value: FEED_RENDER }],
    ])
    const at = out.findIndex(([k]) => 'authors' === k)
    if (0 <= at) out.splice(at, 1)
    out.push(['authors', arr([author])])
  }
  return obj(out)
}

// Whether a grammar spec written in a grammar notation reads back as the
// notations' conventions say: undefined when it does, why not when it does
// not. A render writes the spec anew, as far as its notation can say it,
// and its contract is the round trip (each render's header): the text
// compiles back to the spec it was written from, but where its loss list
// says it compiles back to another (a spec whose alternatives the compiler
// reordered, or whose left recursion ran through another rule), and a spec
// compiled from another notation, which compiles back under the target's
// own settings (its group tag, its lexing, its spelling of the other
// notation's terminals and core rules) and recognises what it recognised.
// So a spec of the target's own notation reads back as it was, or as one
// the render writes again as the same text; and one of another notation
// reads back as a spec the render writes again as text that reads back as
// that spec.
function checkGrammar(from: Format, target: Format, source: Datum, written: string): string | undefined {
  const limits = Limits.default()
  let back: Datum
  try {
    back = target.read(written, limits)
  } catch (err) {
    return `the written grammar does not read back: ${why(err)}`
  }
  if (from.id === target.id && same(source, back)) return undefined
  let again: string
  try {
    again = translateText(target, target, written)
  } catch (err) {
    return `the spec it reads back as is not written again: ${why(err)}; it was written as ${JSON.stringify(written)}`
  }
  if (from.id === target.id) {
    return again === written
      ? undefined
      : `reads back as another spec, which is written again as ${JSON.stringify(again)}, where ` +
          `${JSON.stringify(written)} was written`
  }
  let backAgain: Datum
  try {
    backAgain = target.read(again, limits)
  } catch (err) {
    return `the grammar written again does not read back: ${why(err)}`
  }
  return same(back, backAgain)
    ? undefined
    : `reads back as a spec that is written again as ${JSON.stringify(again)}, which reads back as another, ` +
        `where ${JSON.stringify(written)} was written`
}

// Whether a grammar spec sets the lexing GBNF's compiler gives a spec:
// exact, no white space skipped and no matcher of the engine's own
// (`space.lex` off).
function exactLexing(spec: Datum): boolean {
  const member = (d: Datum | undefined, key: string): Datum | undefined =>
    undefined !== d && 'object' === d.type ? d.members.get(key) : undefined
  const lex = member(member(member(spec, 'options'), 'space'), 'lex')
  return undefined !== lex && 'bool' === lex.type && false === lex.value
}

// The engine with a grammar spec installed, as a fresh instance each:
// installing applies the spec's lexer options to the instance.
function grammarEngine(spec: Datum): Tabnas {
  const engine = new Tabnas()
  engine.grammar(JSON.parse(toText(spec)))
  return engine
}

// Whether an engine parses a sample.
function accepts(engine: Tabnas, sample: string): boolean {
  try {
    engine.parse(sample)
    return true
  } catch (_err) {
    return false
  }
}

// What a grammar spec, `source`, written in `target` as `written`
// recognises against what the spec that text compiles to recognises, over
// the document's samples: each sample is parsed with both, and both accept
// it or both refuse it. A render writes a spec as far as its notation can
// say it and recognises what it recognised (each loss list's sentence on
// the tree builders), but for the lexing: a spec of another notation
// compiles back under the target's own settings (its loss list), so across
// GBNF's exact lexing and the others' default one, which skips white
// space, a sample holding white space may be recognised otherwise, as
// declared; the caller holds those to the samples
// `test/notation-samples.json` registers for the pair. The number of
// samples compared and the samples recognised otherwise across the lexing,
// or why not.
function recognition(
  target: Format,
  source: Datum,
  written: string,
  samples: ReadonlyArray<string>,
): { compared: number; otherwise: string[] } | string {
  if (0 === samples.length) return { compared: 0, otherwise: [] }
  let back: Datum
  try {
    back = target.read(written, Limits.default())
  } catch (err) {
    return `the written grammar does not read back: ${why(err)}`
  }
  const across = exactLexing(source) !== exactLexing(back)
  let read: Tabnas
  let writtenEngine: Tabnas
  try {
    read = grammarEngine(source)
    writtenEngine = grammarEngine(back)
  } catch (err) {
    return `the spec does not install: ${why(err)}`
  }
  const otherwise: string[] = []
  for (const sample of samples) {
    const was = accepts(read, sample)
    const is = accepts(writtenEngine, sample)
    if (was === is) continue
    if (across && /[ \t\n\r]/.test(sample)) {
      otherwise.push(sample)
      continue
    }
    return (
      `recognises ${JSON.stringify(sample)} otherwise: the grammar read ${was ? 'accepts' : 'refuses'} it, ` +
      `the one written ${is ? 'accepts' : 'refuses'} it, where ${JSON.stringify(written)} was written`
    )
  }
  return { compared: samples.length, otherwise }
}

// Whether the document read back from `written` in `target` is what the
// target's conventions make of `source`, read as `from`: undefined when it
// is, why not when it is not.
function check(from: Format, target: Format, source: Datum, written: string): string | undefined {
  if (grammarNotation(target)) return checkGrammar(from, target, source, written)
  const limits = Limits.default()
  const id = target.id
  // A source whose events carry the target's own schema (XML's element
  // tree into XML) is written as it is, and reads back as it is.
  const embedded = undefined !== target.part.schema && from.part.schema !== target.part.schema
  let back: Datum
  if ('markdown' === id) {
    // A Markdown table reads back as records through the format's lift, as
    // a host reads it for a records target.
    const lift = target.part.lift
    assert.ok(lift, 'markdown has a lift')
    const program = `${lift.text}\ndef export [input] (records (${lift.entry} input))\n`
    let json: string
    try {
      json = translateText(target, format('yaml'), written, { file: 'markdown-records.alc', text: program })
    } catch (err) {
      return `the written table does not read back: ${why(err)}`
    }
    try {
      back = format('yaml').read(json, limits)
    } catch (err) {
      return `its records do not read back: ${why(err)}`
    }
  } else if (('xml' === id || 'feed' === id) && embedded) {
    // An embedding reads back through its reverse, which its file holds
    // beside it (`xml-unembed`, `feed-unembed`): the format's tree, as JSON,
    // unembedded, and written where a non-finite number has a spelling.
    let tree: Datum
    try {
      tree = target.read(written, limits)
    } catch (err) {
      return `the written document does not read back: ${why(err)}`
    }
    const embed = target.part.embed
    assert.ok(embed, 'an embedding target has an embed')
    assert.ok(embed.entry.endsWith('-embed'), "an embed's entry is named NAME-embed")
    const reverse = `${embed.entry.slice(0, -'-embed'.length)}-unembed`
    const program = `${embed.text}\ndef export [input] (${reverse} input)\n`
    let yaml: string
    try {
      yaml = translateText(format('json'), format('yaml'), toText(tree), { file: `${reverse}.alc`, text: program })
    } catch (err) {
      return `the tree does not unembed: ${why(err)}`
    }
    try {
      back = format('yaml').read(yaml, limits)
    } catch (err) {
      return `the unembedded tree does not read back: ${why(err)}`
    }
  } else {
    try {
      back = target.read(written, limits)
    } catch (err) {
      return `the written document does not read back: ${why(err)}`
    }
  }
  const key = alchemyTranslate.Options.default().key
  let expected: Datum
  switch (id) {
    case 'csv':
      return checkRecords(source, back, (t) => t)
    case 'markdown':
      return checkRecords(source, back, markdownCell)
    case 'ini': {
      const wrapped = wrapObject(source, key)
      return iniSame(wrapped, back) ? undefined : `read back as ${toText(back)}, where ${toText(wrapped)} was written`
    }
    case 'json':
    case 'jsonc':
    case 'jsonic':
      expected = mapNonFinite(source, () => NULL)
      break
    case 'jsonl':
      expected = mapNonFinite(wrapArray(source), () => NULL)
      break
    case 'toml':
      expected = withoutNulls(wrapObject(source, key))
      break
    case 'zon':
      expected = zonReading(source)
      break
    case 'expr':
      expected = exprReading(source)
      break
    case 'semver':
      expected = semverReading(source)
      break
    case 'feed':
      // A feed's own tree, written as Atom 1.0, with no member whose value
      // is null, so a null member read back is one left out (an embedded
      // tree reads back through its reverse, above, as it was).
      if (!embedded) {
        expected = feedReading(source)
        back = feedClean(back)
      } else expected = source
      break
    default:
      expected = source
  }
  const read = 'zon' === id ? zonBack(back) : back
  return same(expected, read) ? undefined : `read back as ${toText(read)}, where ${toText(expected)} was written`
}

// How deep a value nests: a scalar is 0, a container one more than its
// deepest member.
function depth(d: Datum): number {
  if ('array' === d.type) return 1 + d.items.reduce((m, i) => Math.max(m, depth(i)), 0)
  if ('object' === d.type) return 1 + [...d.members.values()].reduce((m, v) => Math.max(m, depth(v)), 0)
  return 0
}

// The nesting every format reads: the readers guard nesting at different
// depths (tabnas-json past 128 levels, YAML's and ZON's near it, the
// transducer at 256 events deep, which XML's embedding reaches at about
// 127 levels, two elements a level), and a root adapter or an embedding
// adds a level or two. A document nested deeper than this is at one
// format's guard and past another's, a limit and not a shape, so the
// matrix leaves it out, and pins how few such documents there are.
const DEPTH_BOUND = 100

// How many values a tree holds: a scalar is one, a container one more than
// its members hold.
function size(d: Datum): number {
  if ('array' === d.type) return 1 + d.items.reduce((n, i) => n + size(i), 0)
  if ('object' === d.type) return 1 + [...d.members.values()].reduce((n, v) => n + size(v), 0)
  return 1
}

// The size of a tree every format writes in moments. The grammar spec RFC
// 3986's URI grammar compiles to holds 513,409 values (its probe tables),
// which take seconds into JSON and minutes into an XML embedding, and this
// command's interpreter takes minutes over the 14,000 values the larger
// GBNF examples compile to (C's, JSON's): a workload and not a shape, so
// the matrix leaves a document past this out, and pins how few such
// documents there are (the Rust crate's release run, faster, reads trees
// ten times this size).
const SIZE_BOUND = 10_000

// What a run of the matrix is held to: at least `floor` documents, at most
// `tooDeep` of them deeper than every format reads and `tooLarge` larger
// than every format writes in moments, at least `refusals` pairs refused as
// their target declares, at least `grammarsWritten` grammars written in a
// grammar notation, and at least `samplesCompared` samples of theirs
// (NOTATION's `samples`) compared between the grammar read and the one
// written, each recognised by both alike but those NOTATION's `otherwise`
// registers for the pair.
type Bounds = {
  floor: number
  tooDeep: number
  tooLarge: number
  // The documents a source's part refuses part way, at most: each is read
  // with --whole, and its pairs are held to what that writes.
  whole: number
  refusals: number
  grammarsWritten: number
  samplesCompared: number
}

// The cross product of `docs` and every format: each document read with
// its format's grammar, written in every format, and read back under the
// target's conventions, or refused as the target declares, held to
// `bounds`; a document of a format whose reader is a registered defect is
// left out as a source, and counted.
function matrix(docs: Doc[], bounds: Bounds): void {
  const limits = Limits.default()
  const targets = formats()
  assert.equal(targets.length, 22, `the formats: ${names()}`)
  const total = docs.length * targets.length
  const failures: string[] = []
  const refusedSources: string[] = []
  const tooDeep: string[] = []
  const tooLarge: string[] = []
  const defective: string[] = []
  const refusals: string[] = []
  const readWhole = new Map<string, string>()
  let grammarsWritten = 0
  let samplesCompared = 0
  const declaredOtherwise: string[] = []
  let pairs = 0
  const started = Date.now()
  let reported = Date.now()
  docs.forEach(({ name, id, text }, n) => {
    const from = format(id)
    let source: Datum | undefined
    if (registeredDefect(from.id)) {
      defective.push(name)
    } else {
      try {
        source = from.read(text, limits)
      } catch (err) {
        refusedSources.push(`${name}: ${why(err)}`)
      }
      if (undefined !== source && depth(source) > DEPTH_BOUND) {
        tooDeep.push(`${name}: ${depth(source)} levels`)
        source = undefined
      } else if (undefined !== source && size(source) > SIZE_BOUND) {
        tooLarge.push(`${name}: ${size(source)} values`)
        source = undefined
      }
    }
    for (const to of undefined === source ? [] : targets) {
      pairs += 1
      const pair = `${name} (${from.id}) -> ${to.id}`
      const held = expect(from, to, source as Datum)
      let failure: string | undefined
      let written: string | undefined
      let refused: unknown
      try {
        written = translateText(from, to, text)
      } catch (err) {
        refused = err
      }
      // A document the source's part cannot stream fails, naming --whole;
      // read whole, the pair is held to what that writes, and the document
      // is counted.
      if (undefined === written && unstreamed(refused)) {
        readWhole.set(`${name} (${from.id})`, String((refused as { code?: string }).code))
        refused = undefined
        try {
          written = translateText(from, to, text, undefined, true)
        } catch (err) {
          refused = err
        }
      }
      if (!held.written) {
        const fail = refused as { code?: string; message?: string } | undefined
        if (undefined !== written) {
          failure = `is written, where it is declared refused (${held.code}, ${held.reason}...): ${JSON.stringify(written)}`
        } else if (held.code === fail?.code && String(fail?.message).startsWith(held.reason)) {
          refusals.push(`${pair}: ${held.reason}`)
        } else {
          failure = `is refused otherwise than declared (${held.code}, ${held.reason}...): ${why(refused)}`
        }
      } else if (undefined === written) {
        const fail = refused as { code?: string; message?: string } | undefined
        if (undefined !== held.unless && held.unless.code === fail?.code && String(fail?.message).startsWith(held.unless.reason)) {
          refusals.push(`${pair}: ${String(fail?.message)}`)
        } else {
          failure = `does not write: ${why(refused)}`
        }
      } else if ('records' === from.part.reads[0] && 'records' === to.part.writes) {
        // A records source read through its lift writes its table, not its
        // tree: compare with the table.
        try {
          to.read(written, limits)
        } catch (err) {
          failure = `the written document does not read back: ${why(err)}`
        }
      } else {
        if (undefined !== held.unless) grammarsWritten += 1
        try {
          failure = check(from, to, source as Datum, written)
          if (undefined === failure && undefined !== held.unless) {
            const recognised = recognition(to, source as Datum, written, NOTATION.samples[name] ?? [])
            if ('string' === typeof recognised) failure = recognised
            else {
              samplesCompared += recognised.compared
              const registered = NOTATION.otherwise[pair] ?? []
              if (JSON.stringify(recognised.otherwise) !== JSON.stringify(registered)) {
                failure =
                  `recognises ${JSON.stringify(recognised.otherwise)} otherwise across the lexing, where ` +
                  `test/notation-samples.json registers ${JSON.stringify(registered)} for the pair`
              } else if (0 < recognised.otherwise.length) {
                declaredOtherwise.push(`${pair}: ${recognised.otherwise.length} of ${recognised.compared} samples`)
              }
            }
          }
        } catch (err) {
          failure = `the check failed: ${why(err)}`
        }
      }
      if (undefined !== failure) failures.push(`${pair}: ${failure}`)
    }
    if ((n + 1) % 25 === 0 || n + 1 === docs.length || Date.now() - reported > 20_000) {
      reported = Date.now()
      process.stderr.write(
        `matrix: ${n + 1} of ${docs.length} documents (${Math.floor(((n + 1) * 100) / docs.length)}%), ` +
          `${failures.length} failures, ${Math.floor((Date.now() - started) / 1000)}s\n`,
      )
    }
  })
  for (const line of refusedSources) process.stderr.write(`refused source: ${line}\n`)
  for (const line of tooDeep) process.stderr.write(`deeper than every format reads: ${line}\n`)
  for (const line of tooLarge) process.stderr.write(`larger than ${SIZE_BOUND} values: ${line}\n`)
  for (const [doc, code] of readWhole) process.stderr.write(`read with --whole, which its part cannot stream: ${doc}: ${code}\n`)
  for (const line of declaredOtherwise) {
    process.stderr.write(
      'recognised otherwise across the lexing, as the loss lists declare and test/notation-samples.json ' +
        `registers: ${line} holding white space\n`,
    )
  }
  for (const line of failures) process.stderr.write(`FAIL ${line}\n`)
  const schemaOnly = refusals.filter((r) => r.includes('schema_only:')).length
  const unwritable = refusals.filter((r) => r.includes(': the grammar spec cannot be written as ')).length
  const exprRefused = refusals.filter((r) => r.includes(': the object at ')).length
  process.stderr.write(
    `matrix: ${pairs} pairs of ${docs.length} documents; ${refusedSources.length} refused by their own ` +
      `reader, ${tooDeep.length} too deep, ${tooLarge.length} too large, ${defective.length} left out for a ` +
      `registered reader defect; ${readWhole.size} read with --whole, which their parts cannot stream; ` +
      `${refusals.length} pairs refused as their target declares (${schemaOnly} by a schema-only target, ` +
      `${unwritable} by a grammar notation's render, ${exprRefused} by expr's render, ` +
      `${refusals.length - schemaOnly - unwritable - exprRefused} by Semantic ` +
      `Versioning's embedding); ${grammarsWritten} grammars written in a grammar notation, ${samplesCompared} of ` +
      `their samples compared, ${declaredOtherwise.length} pairs recognising some otherwise across the lexing, ` +
      `as registered\n`,
  )
  const leftOut = refusedSources.length + tooDeep.length + tooLarge.length + defective.length
  assert.ok(
    pairs + leftOut * targets.length === total && docs.length >= bounds.floor,
    `the corpus shrank: ${docs.length} documents`,
  )
  assert.ok(tooDeep.length <= bounds.tooDeep, `${tooDeep.length} documents are deeper than every format reads (above)`)
  assert.ok(
    readWhole.size <= bounds.whole,
    `${readWhole.size} documents are read with --whole, more than the ${bounds.whole} the corpus gives (above)`,
  )
  assert.ok(
    tooLarge.length <= bounds.tooLarge,
    `${tooLarge.length} documents hold more than ${SIZE_BOUND} values (above)`,
  )
  assert.ok(
    refusals.length >= bounds.refusals,
    `${refusals.length} pairs are refused as their target declares, fewer than the ${bounds.refusals} the corpus gives`,
  )
  assert.ok(
    samplesCompared >= bounds.samplesCompared,
    `${samplesCompared} samples are compared, fewer than the ${bounds.samplesCompared} the corpus gives`,
  )
  assert.ok(
    grammarsWritten >= bounds.grammarsWritten,
    `${grammarsWritten} grammars are written in a grammar notation, fewer than the ${bounds.grammarsWritten} the ` +
      `corpus gives`,
  )
  assert.ok(0 === failures.length, `${failures.length} of ${pairs} pairs failed (above)`)
}

// A request from `from` to `to`, with no path and no program.
function request(from: Format, to: Format): Request {
  return { from, to, options: alchemyTranslate.Options.default(), program: undefined, limits: Limits.default() }
}

function named(id: string): Format {
  const found = formatNamed(id)
  assert.ok(undefined !== found, `${id} is a format`)
  return found
}

describe('translate', () => {
  // transduce's fixtures, one document per format at least, and the
  // documents of JSONTestSuite every JSON parser must accept.
  it('every document translates into every format', () => {
    matrix(corpus(), { floor: 151, tooDeep: 0, tooLarge: 4, whole: 12, refusals: 1084, grammarsWritten: 36, samplesCompared: 236 })
  })

  // Each registered reader defect still stands: a version past 2^53 - 1
  // does not survive its own round trip. Once a reader is repaired this
  // fails, until its entry in READER_DEFECTS is deleted.
  it('a registered reader defect still stands', () => {
    for (const [id, defect] of READER_DEFECTS) {
      if ('semver' === id) {
        const text = '99999999999999999999.1.2'
        const back = translateText(named(id), named(id), text)
        assert.notStrictEqual(back, text, `${id} now keeps a version past 2^53 - 1: delete its entry (${defect})`)
      } else {
        assert.fail(`${id}: a registered defect needs an example here`)
      }
    }
  })

  // An expression is read as expr's API reads one for its shared fixtures
  // (parseSimplified), the tree its parts declare: each operation a list
  // whose first element is the operator's source text, as the Rust and Go
  // readers read it, where its parser's value holds the object that
  // describes the operator, token and all.
  it('an expression is read as the simplified tree its package builds', () => {
    const text = '1+2*(3-x)\n'
    const read = named('expr').read(text, Limits.default())
    assert.strictEqual(toText(read), '["+",1,["*",2,["(",["-",3,"x"]]]]')
    assert.deepStrictEqual(JSON.parse(toText(read)), expr.parseSimplified(new Tabnas().use(jsonic).use(expr.Expr), text))
    assert.strictEqual(translateText(named('expr'), named('json'), text), '["+",1,["*",2,["(",["-",3,"x"]]]]\n')
  })

  // feed's TypeScript reader builds a person's absent uri and email as
  // null, where its Rust and Go readers leave them out, so a translation
  // from a feed read whole (--whole, the reader's value) writes them as null
  // in this runtime alone. feed's transducer part, from the package that
  // ships one, writes what the Rust and Go readers build. feed's render
  // writes no member whose value is null, so the matrix reads a written feed
  // back with its null members left out. This fails once the readers agree,
  // and is then deleted.
  it("feed's reader builds a person's absent members as null", () => {
    const text =
      '<feed xmlns="http://www.w3.org/2005/Atom"><title>T</title><id>i</id>' +
      '<updated>2003-12-13T18:30:02Z</updated><author><name>A</name></author></feed>'
    const json = translateText(named('feed'), named('json'), text, undefined, true)
    assert.ok(json.includes('"authors":[{"name":"A","uri":null,"email":null}]'), json)
  })

  // ZON's reader builds an integer no double holds exactly as a bigint,
  // which this command reads as the object {"$big": "<digits>"}, the
  // digits after a minus sign when it is negative, as ZON's part declares
  // and its Rust reader builds; ZON's render writes it back as the
  // integer.
  it('a ZON integer no double holds is the $big object', () => {
    const [zon, json] = [named('zon'), named('json')]
    const text = '.{ 1, 12345678901234567890, -12345678901234567890, 0xc1ce108124179e16 }\n'
    const want =
      '[1,{"$big":"12345678901234567890"},{"$big":"-12345678901234567890"},{"$big":"13965117641364839958"}]'
    assert.strictEqual(translateText(zon, json, text).trimEnd(), want)
    assert.strictEqual(translateText(zon, json, translateText(zon, zon, text)).trimEnd(), want)
  })

  // The metrics a run is given count what it wrote, whether it streamed
  // through the transducer part or read the document whole (--whole), and
  // a run the part gives up writes nothing twice: it fails, its metrics
  // what it had written.
  it("a run's metrics are its own", () => {
    const json = named('json')
    for (const [text, whole] of [
      ['{"a": [1, 2]}', false],
      ['{"a": 1, "a": 2}', true],
    ] as const) {
      const out = new BytesWriter()
      const metrics = new Metrics()
      const r = { ...request(json, json), whole }
      runCompiled(r, compile(r), text, out, metrics)
      const written = out.bytes().length
      assert.ok(0 < written, text)
      assert.strictEqual(metrics.output_bytes, written, text)
      assert.ok(0 < metrics.events, text)
    }
    const out = new BytesWriter()
    const metrics = new Metrics()
    const r = request(json, json)
    assert.throws(
      () => runCompiled(r, compile(r), '{"a": 1, "a": 2}', out, metrics),
      (err: { code?: string }) => 'DUPLICATE_MEMBER' === err.code,
    )
    assert.strictEqual(metrics.output_bytes, out.bytes().length)
  })

  // A writer that fails once it has taken some of the output: the failure
  // says the output had left, and says it had not when it took none.
  it('an output failure after bytes left is committed', () => {
    const json = named('json')
    for (const [takes, committed] of [
      [0, false],
      [3, true],
    ] as const) {
      let left: number = takes
      const out: Writer = {
        write(bytes: Uint8Array): number {
          if (0 === left) throw new Error('the disk is full')
          const n = Math.min(left, bytes.length)
          left -= n
          return n
        },
      }
      const r = request(json, json)
      assert.throws(
        () => runCompiled(r, compile(r), '[1, 2, 3]', out, new Metrics()),
        (err: { code?: string; committedOutput?: boolean }) =>
          'OUTPUT_FAILED' === err.code && committed === err.committedOutput,
        String(takes),
      )
    }
  })
})
