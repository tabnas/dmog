/* Copyright (c) 2026 tabnas, MIT License */

// Preloaded into the `dmog` command by the streaming gate
// (test/streaming.test.ts), with `node --require`: it measures what the
// command holds as it reads its input and writes its output, and changes
// nothing the command does.
//
// What the command holds is the live heap after a full collection, less
// the code V8 has compiled, and the memory outside the heap that buffers
// hold (`process.memoryUsage()`'s `heapUsed` and `external`, which counts
// the array buffers): a document read whole is held in buffers before it
// is a string. The compiled code is the heap's code and trusted spaces
// (`v8.getHeapSpaceStatistics()`), machine code and bytecode: it is bounded
// by the command's own code, not by its document, and it grows through a
// run as V8 compiles more of the command, by 0.5 MB to 1.1 MB over the
// gate's documents, which would bury what a part holds of its document.
//
// It is sampled when the command first opens or reads INPUT, its last
// argument, and then at every read of it and every write of the output:
// at fixed points in the stream. A write retried after EAGAIN is the same
// point, and is not sampled again. The run's peak is the most a later
// sample holds past the least any sample holds. The peak resident set
// (`process.resourceUsage().maxRSS`) would not do: V8 sizes its heap to
// what the part's interpreter allocates, near 200 MB over 300 items of JSON
// and over 3,000 alike (190 MB and 213 MB), of which the run holds about
// 1 MB.
//
// The gate runs the command with V8's background threads off
// (`--single-threaded`), so that every run of a document samples alike, on
// a loaded machine as on an idle one. With them on, what a collection finds
// alive depends on whether an optimization compiled on another thread has
// been installed yet, which takes longer on a busy machine, and a sample
// taken before it is can hold what is garbage otherwise: a heap snapshot of
// one such sample found a finished parse's rules and tokens. On four busy
// cores, one of 82 runs of the gate's documents held 9.5 MB more when it
// opened its input, and another 1.3 MB more at its first read; with the
// compiles delayed 150 ms (`--concurrent-recompilation-delay`), three of
// six runs of feed over 3,000 items held 9.5 MB more at their first read,
// and none did with them on the main thread. A sample swollen at the start
// leaves the least at a later one, past the parser the run has built, so
// the peak falls short; one swollen later raises it.
//
// At exit it writes `{ peak, over, samples }` to the file
// DMOG_STREAMING_MEMORY names: `peak` is the run's peak, in bytes. Once
// that passes DMOG_STREAMING_LIMIT bytes, where it is set, it writes
// `over: true` and ends the command, which has shown what the gate asks of
// it.

// The module object itself, not a copy of it, so that what is patched here
// is what the command calls.
import fs = require('node:fs')
import { getHeapSpaceStatistics, setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'

setFlagsFromString('--expose_gc')
const gc = runInNewContext('gc') as () => void

const report = process.env.DMOG_STREAMING_MEMORY
const limit = Number(process.env.DMOG_STREAMING_LIMIT ?? '0')
const input = process.argv[process.argv.length - 1]

// The heap spaces that hold compiled code: `code_space`, `trusted_space`
// (bytecode among it), and their shared and large-object spaces.
const COMPILED = /code|trusted/

let begun = false
let least = Infinity
let most = -Infinity
let samples = 0
let over = false
let reporting = false
let retrying = false

function held(): number {
  gc()
  const usage = process.memoryUsage()
  let compiled = 0
  for (const space of getHeapSpaceStatistics()) {
    if (COMPILED.test(space.space_name)) compiled += space.space_used_size
  }
  return usage.heapUsed - compiled + usage.external
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
  if (!retrying) sample()
  try {
    const written = (writeSync as (...args: unknown[]) => unknown).apply(this, args)
    retrying = false
    return written
  } catch (err) {
    retrying = 'EAGAIN' === (err as NodeJS.ErrnoException)?.code
    throw err
  }
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
