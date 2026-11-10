/* Copyright (c) 2026 tabnas, MIT License */

// The streaming gate (rs/tests/streaming_test.rs in Rust,
// go/cmd/dmog/streaming_test.go in Go). Every format the formats table
// lists is held to the definition of streaming in tabnas/alchemy's
// docs/streaming-plan.md, section 13 (admin ADR-33):
//
//   A format streams, for the gate, when in every runtime a document of it
//   read from a file and from standard input has written its first byte
//   before its input ends, and the run's peak memory stays flat over ten
//   times the document.
//
// The documents are test/streaming.json's: for each format, a document of
// its `items` items and one of ten times as many. Each format is held to
// two clauses:
//
// - written before the end: `dmog translate --from F --to json` is handed
//   the document of `items` items in two parts, from standard input and
//   from a named pipe given as INPUT (a FIFO, which `mkfifo` makes; there
//   is none on Windows, where only standard input is run), and must write
//   to standard output after the first part, which holds nine tenths of the
//   items, before the rest is sent; the run must then end as a run over the
//   whole document does, with the same output;
// - flat memory: what the command holds, over the document of `items`
//   items and over ten times as many, must stay within BOUND of each other.
//   test/streaming-memory.ts, preloaded into the command, measures it: the
//   live heap and the buffers outside it, sampled as the command reads and
//   writes, past what it held when it opened INPUT.
//
// A format that fails either clause is registered in NOT_STREAMING with the
// clauses it fails and why; the gate holds each entry to failing exactly
// those, so a format that starts streaming fails it until its entry is
// deleted. The formats the maintainer ruled whole are listed apart
// (RULED_WHOLE) and are not run.

import { after, describe, it } from 'node:test'
import assert from 'node:assert'
import { spawn, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { closeSync, constants, createWriteStream, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Writable } from 'node:stream'

import { format as named, formats } from '../dist/translate'

// The command, as the package's `bin` names it, and the preload that
// measures it.
const BIN = join(__dirname, '..', 'bin', 'dmog')
const MEMORY = join(__dirname, 'streaming-memory.js')

const TMP = mkdtempSync(join(tmpdir(), 'dmog-streaming-'))

// The clauses of the definition a format fails.
type Fails = 'written' | 'flat' | 'both'

// The formats the maintainer ruled whole (admin ADR-33, ruling 4: "expr and
// semver stay whole"): their documents are read whole by design, as their
// `whole` sentence in the formats table says, and the gate does not run
// them.
const RULED_WHOLE: Record<string, string> = {
  expr: "an expression's tree is complete only at its end; admin ADR-33, ruling 4",
  semver: 'a version is made only once its whole text is accepted; admin ADR-33, ruling 4',
}

// The formats that do not stream yet in this runtime, each with the clauses
// it fails and why. The gate holds each entry to failing exactly those
// clauses: delete an entry, or a clause of it, once its format passes.
const NOT_STREAMING: Record<string, { fails: Fails; why: string }> = {
  abnf: {
    fails: 'both',
    why:
      'it has no transducer part: a document is read whole as the grammar spec its compiler writes, ' +
      "until abnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
  },
  c: {
    fails: 'both',
    why:
      "it has no transducer part: a translation unit is read whole as the parser's value (admin " +
      "ADR-33, ruling 7; the streaming plan's step 21, whose ruling is pending)",
  },
  css: {
    fails: 'flat',
    why: "css is not in transduce's prune suite, so the engine's tree keeps every rule (tabnas/transduce#82)",
  },
  ebnf: {
    fails: 'both',
    why:
      'it has no transducer part: a document is read whole as the grammar spec its compiler writes, ' +
      "until ebnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
  },
  gbnf: {
    fails: 'both',
    why:
      'it has no transducer part: a document is read whole as the grammar spec its compiler writes, ' +
      "until gbnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)",
  },
  ini: {
    fails: 'flat',
    why:
      "every section is a member of the document's root map, and pruning empties no map, so the " +
      "engine's tree keeps every section (tabnas/transduce#81)",
  },
  jsonic: {
    fails: 'both',
    why:
      "jsonic's constructor declares no lex.window, so the engine reads a document whole before its " +
      'parse begins (tabnas/jsonic#121)',
  },
  markdown: {
    fails: 'flat',
    why:
      "a document's blocks array is no rule's node, so pruning never empties it, and each block's node " +
      'stays (tabnas/transduce#81)',
  },
  proto: {
    fails: 'both',
    why:
      "the check of its nesting that proto's parse makes first counts the whole document, and proto's " +
      'grammar declares no lex.window, so a document is read whole first (tabnas/proto#81)',
  },
  toml: {
    fails: 'flat',
    why:
      "an array of tables is no rule's node, so pruning never empties it, and each table stays whole " +
      '(tabnas/transduce#81)',
  },
}

// How far what a run holds over ten times the document may pass what it
// holds over the document: 1.5 times it. Measured with test/streaming.json's
// documents (300 items, about 315 KB, and 3,000), the formats that stream
// hold at most a quarter more at ten times, what more samples catch of the
// window's swing and of the code V8 optimizes as a run goes on: json 1.26 MB
// and 1.53 MB, jsonl 1.29 MB and 1.58 MB, feed 1.69 MB and 2.06 MB, csv
// 1.70 MB and 1.81 MB, yaml 1.94 MB and 1.83 MB. A format that holds
// something of its document grows with it, run to its end: jsonic, read
// whole, 1.58 MB and 6.98 MB, css 1.87 MB and 6.45 MB, ini, whose sections
// stay in the engine's tree, 2.14 MB and 6.65 MB, and the least, markdown,
// which keeps a node of every block, 1.70 MB and 3.23 MB (1.9 times). JSON
// read whole (`--whole`) holds 0.92 MB and 7.73 MB (8.4 times), which 'a
// document held whole fails the bound' holds to fail it.
const BOUND = 1.5

// A format's document in test/streaming.json: `head`, then `item` once for
// each index with `{i}` the index, `separator` between two items, and
// `tail`; `{text}` in any of them is the file's `text`.
type Template = { head: string; item: string; separator: string; tail: string }

const STREAMING: { items: number; text: string; documents: Record<string, Template> } = JSON.parse(
  readFileSync(join(__dirname, '..', '..', 'test', 'streaming.json'), 'utf8'),
)

const ITEMS = STREAMING.items

function template(id: string): Template {
  const t = STREAMING.documents[id]
  const fill = (s: string): string => s.split('{text}').join(STREAMING.text)
  return { head: fill(t.head), item: fill(t.item), separator: fill(t.separator), tail: fill(t.tail) }
}

// The document of `n` items in two parts: the head and the first `split`
// items, and the rest.
function parts(t: Template, n: number, split: number): [string, string] {
  const items = Array.from({ length: n }, (_, i) => t.item.split('{i}').join(String(i)))
  const first = t.head + items.slice(0, split).join(t.separator)
  const rest = (0 < split && split < n ? t.separator : '') + items.slice(split).join(t.separator) + t.tail
  return [first, rest]
}

function documentFile(id: string, n: number): string {
  const path = join(TMP, `${id}-${n}`)
  const [first, rest] = parts(template(id), n, n)
  writeFileSync(path, first + rest)
  return path
}

// What a run wrote: its length and a hash of its bytes.
type Output = { length: number; hash: string }

// One run of the command over a file, with what the preload measured.
type Measured = { peak: number; over: boolean; took: number; output: Output }

const mb = (bytes: number): string => `${(bytes / 1048576).toFixed(2)} MB`

// `dmog translate --from id --to json [--whole] path`, measured, and ended
// once it holds more than `limit` bytes where one is given.
async function measure(id: string, path: string, limit = 0, whole = false): Promise<Measured> {
  const report = `${path}.memory.json`
  const args = ['--require', MEMORY, BIN, 'translate', '--from', id, '--to', 'json']
  const child = spawn(process.execPath, [...args, ...(whole ? ['--whole'] : []), path], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, DMOG_STREAMING_MEMORY: report, DMOG_STREAMING_LIMIT: String(limit) },
  })
  const started = Date.now()
  const hash = createHash('sha1')
  let length = 0
  let errors = ''
  child.stdout.on('data', (data: Buffer) => {
    hash.update(data)
    length += data.length
  })
  child.stderr.on('data', (data: Buffer) => (errors += data.toString('utf8')))
  const [status] = await Promise.all([
    new Promise<number | null>((resolve) => child.once('exit', (code) => resolve(code))),
    new Promise<void>((resolve) => child.stdout.once('end', () => resolve())),
  ])
  const took = (Date.now() - started) / 1000
  const memory = JSON.parse(readFileSync(report, 'utf8'))
  if (!memory.over) assert.equal(status, 0, `${id} ${path}: ${errors}`)
  return { peak: memory.peak, over: memory.over, took, output: { length, hash: hash.digest('hex') } }
}

// What one run over a document handed over in two parts did.
type Fed = { beforeTheEnd: boolean; waited: number; status: number | null; output: Output; stderr: string }

// Whether this platform has FIFOs: `mkfifo` is POSIX, so Windows runs
// standard input alone.
const FIFOS = 'win32' !== process.platform

// Run `dmog translate --from id --to json` over a document handed over in
// two parts, from standard input or from the FIFO `fifo`: `first`, then,
// once the command has written or `wait` seconds have passed, `rest`, and
// the end of the input. Standard output is read throughout, so a command
// that writes as it reads never waits on a pipe nobody reads.
async function feedInTwo(id: string, fifo: string | undefined, first: string, rest: string, wait: number): Promise<Fed> {
  const child = spawn(process.execPath, [BIN, 'translate', '--from', id, '--to', 'json', fifo ?? '-'], {
    stdio: [undefined === fifo ? 'pipe' : 'ignore', 'pipe', 'pipe'],
  })
  const started = Date.now()
  const hash = createHash('sha1')
  const stdout = child.stdout!
  let length = 0
  let errors = ''
  let wrote: () => void = () => undefined
  const firstByte = new Promise<boolean>((resolve) => (wrote = () => resolve(true)))
  stdout.on('data', (data: Buffer) => {
    hash.update(data)
    length += data.length
    wrote()
  })
  child.stderr!.on('data', (data: Buffer) => (errors += data.toString('utf8')))
  const exited = new Promise<number | null>((resolve) => child.once('exit', (code) => resolve(code)))
  const ended = new Promise<void>((resolve) => stdout.once('end', () => resolve()))
  // A FIFO's open waits, on a thread of libuv's, for the command to open it
  // to read. A write to a command that has ended fails, as the run under
  // test may have: that is not the test's failure.
  let opened = undefined === fifo
  const sink: Writable = undefined === fifo ? child.stdin! : createWriteStream(fifo)
  sink.on('open', () => (opened = true))
  sink.on('error', () => undefined)
  sink.write(first)
  let timer: NodeJS.Timeout | undefined
  const late = new Promise<boolean>((resolve) => (timer = setTimeout(() => resolve(false), wait * 1000)))
  const beforeTheEnd = await Promise.race([firstByte, late, exited.then(() => false)])
  clearTimeout(timer)
  const waited = (Date.now() - started) / 1000
  sink.end(rest)
  const status = await exited
  // A command that ended before it opened the FIFO leaves the open
  // waiting: opening the FIFO to read without waiting lets it through, and
  // its writes then fail.
  while (!opened && undefined !== fifo) {
    closeSync(openSync(fifo, constants.O_RDONLY | constants.O_NONBLOCK))
    await new Promise((resolve) => setTimeout(resolve, 10))
  }
  await ended
  return { beforeTheEnd, waited, status, output: { length, hash: hash.digest('hex') }, stderr: errors }
}

// What the gate found of one format.
type Found = {
  written: Array<[string, number | undefined]>
  one: Measured
  ten: Measured
  broken: string[]
}

function failsOf(found: Found): Fails | undefined {
  const written = found.written.every(([, when]) => undefined !== when)
  const flat = !found.ten.over && found.ten.peak <= found.one.peak * BOUND
  return written ? (flat ? undefined : 'flat') : flat ? 'written' : 'both'
}

function summary(found: Found): string {
  const written = found.written.map(([route, when]) =>
    undefined === when
      ? `from ${route} nothing written before the end`
      : `from ${route} written after ${when.toFixed(1)} s`,
  )
  const { one, ten } = found
  return (
    `${written.join(', ')}; peak ${mb(one.peak)} at ${ITEMS} items and ${mb(ten.peak)} at ${ITEMS * 10} ` +
    `(bound ${mb(one.peak * BOUND)}${ten.over ? ', stopped past it' : ''}), ` +
    `${one.took.toFixed(1)} s and ${ten.took.toFixed(1)} s`
  )
}

// Hold one format to both clauses.
async function hold(id: string): Promise<Found> {
  const one = await measure(id, documentFile(id, ITEMS))
  const entry = NOT_STREAMING[id]
  // A command expected to write before the end is given a generous time
  // to; one registered as not writing is given twice the time a whole run
  // took, and two seconds, which a run that wrote as it read would not
  // need.
  const expectWrites = undefined === entry || 'flat' === entry.fails
  const wait = expectWrites ? Math.max(60, 10 * one.took) : 2 * one.took + 2
  const [first, rest] = parts(template(id), ITEMS, Math.floor((ITEMS * 9) / 10))
  const routes: Array<[string, string | undefined]> = [['standard input', undefined]]
  if (FIFOS) {
    const fifo = join(TMP, `${id}.fifo`)
    const made = spawnSync('mkfifo', [fifo])
    assert.equal(made.status, 0, `mkfifo ${fifo}: ${made.stderr}`)
    routes.push(['a FIFO', fifo])
  }
  const fed = await Promise.all(routes.map(([, fifo]) => feedInTwo(id, fifo, first, rest, wait)))
  const broken: string[] = []
  const written: Array<[string, number | undefined]> = []
  routes.forEach(([route], i) => {
    const f = fed[i]
    if (0 !== f.status) broken.push(`from ${route}: status ${f.status}: ${f.stderr.trim()}`)
    else if (f.output.length !== one.output.length || f.output.hash !== one.output.hash)
      broken.push(
        `from ${route}: ${f.output.length} bytes written, where the run over the whole document wrote ` +
          `${one.output.length}, or other bytes`,
      )
    written.push([route, f.beforeTheEnd ? f.waited : undefined])
  })
  const ten = await measure(id, documentFile(id, ITEMS * 10), one.peak * BOUND)
  return { written, one, ten, broken }
}

describe('the streaming gate', { concurrency: 3 }, () => {
  // The documents, ten times as many items among them, and the FIFOs go
  // with the run.
  after(() => rmSync(TMP, { recursive: true, force: true }))

  const held = formats()
    .map((f) => f.id)
    .filter((id) => undefined === RULED_WHOLE[id])

  // Every format the formats table lists streams in this runtime, or is
  // registered: in NOT_STREAMING, failing exactly the clauses its entry
  // names, or in RULED_WHOLE. A few formats are held at once.
  for (const id of held) {
    it(id, async () => {
      assert.ok(undefined !== STREAMING.documents[id], `${id} is a format with no document in test/streaming.json`)
      const found = await hold(id)
      const line = summary(found)
      console.log(`streaming gate: ${id}: ${line}`)
      assert.deepEqual(found.broken, [], id)
      const fails = failsOf(found)
      const entry = NOT_STREAMING[id]
      if (undefined === entry) assert.equal(fails, undefined, `${id} does not stream: it fails ${fails} (${line}); register it with why`)
      else if (undefined === fails) assert.fail(`${id} streams now (${line}): delete its entry, ${entry.fails} (${entry.why})`)
      else assert.equal(fails, entry.fails, `${id} fails ${fails}, where its entry says ${entry.fails} (${line}): correct the entry (${entry.why})`)
    })
  }

  // The bound holds a document read whole to fail: JSON read with
  // `--whole`, its grammar's value walked, holds more over ten times the
  // document than the bound lets a run hold.
  it('a document held whole fails the bound', async () => {
    const one = await measure('json', documentFile('json', ITEMS), 0, true)
    const ten = await measure('json', documentFile('json', ITEMS * 10), one.peak * BOUND, true)
    console.log(`streaming gate: json read whole: peak ${mb(one.peak)} at ${ITEMS} items and ${mb(ten.peak)} at ${ITEMS * 10}`)
    assert.ok(
      ten.over || one.peak * BOUND < ten.peak,
      `json read whole held ${mb(one.peak)} at ${ITEMS} items and ${mb(ten.peak)} at ${ITEMS * 10}, within the bound`,
    )
  })

  // The ledgers and the documents name the formats of the table: each
  // entry a format that is run, each ruling a format the table reads
  // whole, and each document a format the gate runs.
  it('the ledgers and the documents are the tables', () => {
    for (const [id, entry] of Object.entries(NOT_STREAMING)) {
      assert.ok(undefined !== named(id), `${id} (${entry.why}) is no format`)
      assert.ok(undefined === RULED_WHOLE[id], `${id} is registered and ruled whole`)
    }
    for (const [id, why] of Object.entries(RULED_WHOLE)) {
      const f = named(id)
      assert.ok(undefined !== f, `${id} (${why}) is no format`)
      assert.ok(
        undefined !== f.part.whole && undefined === f.transducer,
        `${id} is ruled whole (${why}), but the formats table no longer reads it whole`,
      )
    }
    for (const id of Object.keys(STREAMING.documents)) {
      assert.ok(
        undefined !== named(id) && undefined === RULED_WHOLE[id],
        `test/streaming.json has a document for ${id}, which the gate does not run`,
      )
    }
  })
})
