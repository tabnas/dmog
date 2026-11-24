/* Copyright (c) 2026 tabnas, MIT License */

// Generated inputs in the spec's worked-example shape: the documents
// transduce's tests and benches share (transduce/rs/tests/support/mod.rs,
// which rs/tests/stdlib_test.rs and rs/tests/run_test.rs include by path,
// and its TypeScript twin transduce/ts/test/support.ts). A test package
// holds nothing of another's, so the five generators are spelled here, a
// copy kept in step by hand. A copy that drifted would change which
// documents the tests run over, not whether a result is checked: every
// test that uses them compares two paths over the same document.
//
// A record is `{"id":i,"person":{"name":"Person number i"},"account":
// {"balance":D.CC}}`, the JSON document wraps them under
// `response.payload.deep.records` behind `response.metadata.fields`, the
// JSON Lines variant is one record per line, the CSV variant is the
// flattened columns `id,name,balance`, and the YAML variant is the block
// form of the JSON document.

import { join } from 'node:path'

import { FIXTURES_ROOT } from './host'

// The metadata the worked example carries: three columns by path.
export const METADATA =
  '{"fields":[{"title":"Identifier","path":["id"]},{"title":"Full name","path":["person","name"]},{"title":"Balance","path":["account","balance"]}]}'

function cents(i: number): string {
  return String(i % 100).padStart(2, '0')
}

// One record, as compact JSON.
export function record(i: number): string {
  return `{"id":${i},"person":{"name":"Person number ${i}"},"account":{"balance":${i * 7}.${cents(i)}}}`
}

// The worked-example document with `records` records.
export function recordsJson(records: number): string {
  const parts: string[] = []
  for (let i = 0; i < records; i++) parts.push(record(i))
  return `{"response":{"metadata":${METADATA},"payload":{"deep":{"records":[${parts.join(',')}]}}}}`
}

// The records alone, one JSON document per line.
export function recordsJsonl(records: number): string {
  let s = ''
  for (let i = 0; i < records; i++) s += record(i) + '\n'
  return s
}

// The records flattened to `id,name,balance`, with a header line.
export function recordsCsv(records: number): string {
  let s = 'id,name,balance\n'
  for (let i = 0; i < records; i++) s += `${i},Person number ${i},${i * 7}.${cents(i)}\n`
  return s
}

// The worked-example document as block YAML.
export function recordsYaml(records: number): string {
  let s =
    'response:\n  metadata:\n    fields:\n      - title: Identifier\n        path: [id]\n      - title: Full name\n        path: [person, name]\n      - title: Balance\n        path: [account, balance]\n  payload:\n    deep:\n      records:\n'
  for (let i = 0; i < records; i++) {
    s += `        - id: ${i}\n          person:\n            name: Person number ${i}\n          account:\n            balance: ${i * 7}.${cents(i)}\n`
  }
  return s
}

// transduce's test files, vendored into test/fixtures/transduce, which the
// Rust tests read too.
export const TRANSDUCE = join(FIXTURES_ROOT, 'transduce')

// transduce's fixture documents: required, as they are for the Rust
// tests.
export const FIXTURES = join(TRANSDUCE, 'rs', 'tests', 'fixtures')
