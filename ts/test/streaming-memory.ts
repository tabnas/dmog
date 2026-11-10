/* Copyright (c) 2026 tabnas, MIT License */

// Preloaded into the `dmog` command by the streaming gate
// (test/streaming.test.ts), with `node --require`: it measures what the
// command holds as it reads its input and writes its output, and changes
// nothing the command does.
//
// What the command holds is the live heap after a full collection, and the
// memory outside the heap that buffers hold (`process.memoryUsage()`'s
// `heapUsed` and `external`, which counts the array buffers): a document
// read whole is held in buffers before it is a string. It is sampled when
// the command first opens or reads INPUT, its last argument, and at every
// read of it and every write of the output after that. The run's peak is
// the most a later sample holds past the least any sample holds: the first
// sample would be the baseline, but a collection there now and then finds
// objects the command made before it still alive (7.8 MB more than the
// other runs held there, in one run of eighteen run six at a time), and
// later samples hold less than it. The peak resident set
// (`process.resourceUsage().maxRSS`) would not do: V8 sizes its heap to
// what the part's interpreter allocates, near 200 MB over 300 items of
// JSON and over 3,000 alike (190 MB and 213 MB), of which the run holds
// about 1.3 MB.
//
// At exit it writes `{ peak, over, samples }` to the file
// DMOG_STREAMING_MEMORY names: `peak` is the run's peak, in bytes. Once
// that passes DMOG_STREAMING_LIMIT bytes, where it is set, it writes
// `over: true` and ends the command, which has shown what the gate asks of
// it.

// The module object itself, not a copy of it, so that what is patched here
// is what the command calls.
import fs = require('node:fs')
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'

setFlagsFromString('--expose_gc')
const gc = runInNewContext('gc') as () => void

const report = process.env.DMOG_STREAMING_MEMORY
const limit = Number(process.env.DMOG_STREAMING_LIMIT ?? '0')
const input = process.argv[process.argv.length - 1]

let begun = false
let least = Infinity
let most = -Infinity
let samples = 0
let over = false
let reporting = false

function held(): number {
  gc()
  const usage = process.memoryUsage()
  return usage.heapUsed + usage.external
}

function begin(): void {
  if (begun) return
  begun = true
  least = held()
}

function peak(): number {
  return Math.max(0, most - least)
}

function sample(): void {
  if (!begun || reporting) return
  samples++
  const now = held()
  least = Math.min(least, now)
  most = Math.max(most, now)
  if (0 < limit && limit < peak()) {
    over = true
    process.exit(0)
  }
}

const { closeSync, openSync, readFileSync, readSync, writeSync } = fs
const patched = fs as unknown as Record<string, (...args: unknown[]) => unknown>

patched.openSync = function (this: unknown, ...args: unknown[]) {
  if (args[0] === input) begin()
  return (openSync as (...args: unknown[]) => unknown).apply(this, args)
}

patched.readFileSync = function (this: unknown, ...args: unknown[]) {
  const reading = args[0] === input
  if (reading) begin()
  const read = (readFileSync as (...args: unknown[]) => unknown).apply(this, args)
  if (reading) sample()
  return read
}

patched.readSync = function (this: unknown, ...args: unknown[]) {
  if (0 === args[0] && '-' === input) begin()
  sample()
  return (readSync as (...args: unknown[]) => unknown).apply(this, args)
}

patched.writeSync = function (this: unknown, ...args: unknown[]) {
  sample()
  return (writeSync as (...args: unknown[]) => unknown).apply(this, args)
}

// The report is written with the functions as they were, so that writing
// it samples nothing.
process.on('exit', () => {
  reporting = true
  if (undefined === report) return
  const fd = openSync(report, 'w')
  writeSync(fd, JSON.stringify({ peak: peak(), over, samples }))
  closeSync(fd)
})
