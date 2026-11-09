/* Copyright (c) 2026 tabnas, MIT License */

// The `dmog` command (the Rust binary is rs/src/bin/dmog.rs), run by
// `bin/dmog`. It composes the language, `@tabnas/alchemy`, with the
// stages it compiles onto: transduce's `routers` and render's `renderers`,
// passed to `compile`, the tabnas JSON grammar and its transducer part,
// transduce's `ParserEventSource`, and render's `FdWriter`.
//
//   dmog canon FILE       print the program in canonical form
//   dmog format FILE      print the program in layout form
//   dmog check FILE       parse, desugar, resolve, check and build the plan; print nothing and exit 0
//   dmog explain FILE     print the plan report
//   dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
//                         run the program over the JSON document INPUT; with --grammar, over
//                         the rule passes of INPUT parsed as FORMAT (a transducer part's input)
//   dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
//                  [--with PROGRAM] [--max-output-bytes N] INPUT
//                         write INPUT, read as FORMAT, in another format
//   dmog formats          print the formats translate reads and writes, as JSON
//
// `FILE`, `PROGRAM` and `INPUT` may be `-` for standard input (one of them
// per run). Nothing but the answer goes to standard output. A failure is
// the `Fail` as one JSON object on standard error (`code`, `message`, and
// `path`, `limit`, `row`, `col` when they apply, and `output`: `"partial"`
// when bytes had reached standard output). The exit status follows the
// code: 2 for a program that does not read, resolve or check
// (`DSL_PARSE_ERROR`, `DSL_TYPE_ERROR`, `STREAM_REUSED`,
// `STREAMABILITY_UNKNOWN`), for a usage error and for an unreadable file; 1
// for an input or protocol failure (from `check` too, when building the
// plan reaches a `fail`); 5 for `RESOURCE_LIMIT_EXCEEDED`; 3 for
// `OUTPUT_FAILED`; 6 for `ABORTED`. A standard error that cannot be
// written loses the report, not the status.
//
// The Rust command runs on a thread of 64 MiB so the checker's and the
// evaluator's bounds are reached before the stack's end; here they run on
// alchemy's explicit stack (its `trampoline`), so Node's default stack is
// enough.
// `--max-output-bytes` sets the writer's limit; the others are transduce's
// defaults.
//
// `run` reads `INPUT` as JSON through JSON's own transducer part, the one
// `translate` reads a JSON document through: the part makes the
// document's events of the parser's rule passes, and the engine's arrays
// are emptied behind them. With `--grammar FORMAT` it parses `INPUT` with
// that format's grammar through transduce's `ParserEventSource` and
// compiles the program over the parser's rule passes, as a transducer part
// is compiled. A document of another format is `translate --with`'s
// business: it runs the program over `INPUT` read as `--from`. The output
// goes through a coalescing writer to standard output and is flushed once,
// at the end; a failure found after bytes were written says so.
//
// `translate` reads `INPUT` with the grammar of `--from` and writes it in
// `--to`, through the translation parts each format's package exports, as
// `./translate` composes them: any of the formats `dmog formats` lists
// into any other. `--path` takes a JSON array of keys and indexes
// (`["people",0]`) and translates that value instead of the document;
// `--key` names the member a root is wrapped under for a format whose
// document must be an object (`items` by default); `--with` runs a program
// over the input first, as `run` does, and writes its export's events or
// table in `--to`.

import { readFileSync, readSync, writeSync } from 'node:fs'

import {
  CompileOptions,
  Renderer,
  canonical,
  compile,
  format,
  isFail,
  parseFile,
  rendererNamed,
  translate as alchemyTranslate,
} from '@tabnas/alchemy'
import { FdWriter, renderers } from '@tabnas/render'
import { Fail, Limits, Metrics, routers } from '@tabnas/transduce'
import type { ParserSink, Segment, Sink } from '@tabnas/transduce'

import * as translate from './translate'

// What every program is compiled with: transduce's routers and render's
// renderers, the stages alchemy builds a run from.
// VERSION is this package's version. It MUST equal package.json "version":
// admin/publish.sh rewrites both, and test/version.test.ts fails the build
// if they drift. Mirrors `const VERSION` in go/cmd/dmog/main.go and
// `VERSION` in rs/src/bin/dmog.rs.
export const VERSION = '0.2.0'

const OPTIONS: CompileOptions = { routers, renderers }

const USAGE =
  'usage: dmog canon|format|check|explain FILE\n' +
  '       dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT\n' +
  '       dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY] [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT\n' +
  '       dmog formats\n' +
  '       (a FILE may be - for standard input)'

// A failure and the status it exits with: the code's, except that a usage
// error and an unreadable file exit 2 although their code is
// `INPUT_INVALID`, since they are the command line's fault, not the
// document's.
class Exit {
  constructor(
    readonly fail: Fail,
    readonly status: number,
  ) {}

  static of(fail: Fail): Exit {
    switch (fail.code) {
      case 'DSL_PARSE_ERROR':
      case 'DSL_TYPE_ERROR':
      case 'STREAM_REUSED':
      case 'STREAMABILITY_UNKNOWN':
        return new Exit(fail, 2)
      case 'RESOURCE_LIMIT_EXCEEDED':
        return new Exit(fail, 5)
      case 'OUTPUT_FAILED':
        return new Exit(fail, 3)
      case 'ABORTED':
        return new Exit(fail, 6)
      default:
        return new Exit(fail, 1)
    }
  }
}

function usage(): Exit {
  return new Exit(Fail.input(USAGE), 2)
}

// Write all of `text` to the descriptor, offering a non-blocking pipe the
// rest again when it answers `EAGAIN`.
function writeAll(fd: number, text: string): void {
  let bytes = Buffer.from(text, 'utf8')
  while (0 < bytes.length) {
    let n: number
    try {
      n = writeSync(fd, bytes)
    } catch (err: any) {
      if ('EAGAIN' === err?.code) continue
      throw err
    }
    bytes = bytes.subarray(n)
  }
}

// The failure as one JSON object on standard error. A standard error that
// cannot be written (a closed pipe) loses the report, not the status.
function report(fail: Fail): void {
  try {
    writeAll(2, JSON.stringify(fail.toJSON()) + '\n')
  } catch (_err) {
    // The status still says what happened.
  }
}

// All of standard input, waiting out a non-blocking descriptor's `EAGAIN`.
function readStdin(): Buffer {
  const chunks: Buffer[] = []
  const chunk = Buffer.alloc(64 * 1024)
  for (;;) {
    let n: number
    try {
      n = readSync(0, chunk, 0, chunk.length, null)
    } catch (err: any) {
      if ('EAGAIN' === err?.code) continue
      // A closed or absent standard input reads as empty, as Rust's does.
      if ('EOF' === err?.code) break
      throw err
    }
    if (0 === n) break
    chunks.push(Buffer.from(chunk.subarray(0, n)))
  }
  return Buffer.concat(chunks)
}

// A leading byte order mark is kept, as Rust's `read_to_string` and Go's
// `os.ReadFile` keep it: whether a document may begin with one is its
// grammar's to say.
const UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

// A file's text, or standard input's for `-`; a file that cannot be read,
// or that is not UTF-8, is the command line's failure (status 2).
function read(file: string): string {
  let bytes: Buffer
  try {
    bytes = '-' === file ? readStdin() : readFileSync(file)
  } catch (err: any) {
    throw new Exit(Fail.input(`cannot read ${file}: ${err?.message ?? err}`), 2)
  }
  try {
    return UTF8.decode(bytes)
  } catch (_err) {
    throw new Exit(Fail.input(`cannot read ${file}: stream did not contain valid UTF-8`), 2)
  }
}

type RunOptions = {
  // The format whose parser reads INPUT, its rule passes the program's
  // input (`--grammar`); without one the program reads INPUT as a JSON
  // document's events.
  grammar?: translate.Format
  render?: Renderer
  maxOutputBytes: number | null
  program: string
  input: string
}

const U64_MAX = 2n ** 64n - 1n

// The value of `--max-output-bytes`, as Rust reads a `u64`: digits, a
// leading `+` allowed, no larger than 2^64 - 1.
function outputLimit(n: string): number {
  if (!/^\+?[0-9]+$/.test(n) || BigInt(n) > U64_MAX) {
    throw new Exit(Fail.input(`--max-output-bytes takes a number of bytes, not ${JSON.stringify(n)}`), 2)
  }
  return Number(BigInt(n))
}

function parseRunOptions(args: ReadonlyArray<string>): RunOptions {
  let grammar: translate.Format | undefined
  let render: Renderer | undefined
  let maxOutputBytes: number | null = null
  const files: string[] = []
  let i = 0
  while (i < args.length) {
    const arg = args[i]
    if ('--grammar' === arg) {
      const name = args[i + 1]
      if (undefined === name) throw usage()
      grammar = translate.format(name)
      if (undefined === grammar || !grammar.parses()) {
        throw new Exit(
          Fail.input(
            `--grammar takes a format whose documents are parsed (${translate.parserNames()}), ` +
              `not ${JSON.stringify(name)}`,
          ),
          2,
        )
      }
      i += 2
    } else if ('--render' === arg) {
      const name = args[i + 1]
      if (undefined === name) throw usage()
      render = rendererNamed(name)
      if (undefined === render) {
        throw new Exit(Fail.input(`--render takes csv or json, not ${JSON.stringify(name)}`), 2)
      }
      i += 2
    } else if ('--max-output-bytes' === arg) {
      const n = args[i + 1]
      if (undefined === n) throw usage()
      maxOutputBytes = outputLimit(n)
      i += 2
    } else if (arg.startsWith('--')) {
      throw usage()
    } else {
      files.push(arg)
      i += 1
    }
  }
  if (2 !== files.length) throw usage()
  const [program, input] = files
  if ('-' === program && '-' === input) {
    throw new Exit(Fail.input('only one of PROGRAM and INPUT may be - (standard input)'), 2)
  }
  return { grammar, render, maxOutputBytes, program, input }
}

type TranslateOptions = {
  from: translate.Format
  to: translate.Format
  path?: string
  key?: string
  program?: string
  whole: boolean
  maxOutputBytes: number | null
  input: string
}

function parseTranslateOptions(args: ReadonlyArray<string>): TranslateOptions {
  const named = (flag: string, id: string): translate.Format => {
    const found = translate.format(id)
    if (undefined === found) {
      throw new Exit(Fail.input(`${flag} takes ${translate.names()}, not ${JSON.stringify(id)}`), 2)
    }
    return found
  }
  let from: translate.Format | undefined
  let to: translate.Format | undefined
  let path: string | undefined
  let key: string | undefined
  let program: string | undefined
  let whole = false
  let maxOutputBytes: number | null = null
  const files: string[] = []
  let i = 0
  while (i < args.length) {
    const arg = args[i]
    const value = (): string => {
      const v = args[i + 1]
      if (undefined === v) throw usage()
      return v
    }
    if ('--whole' === arg) {
      whole = true
      i += 1
      continue
    }
    if ('--from' === arg) from = named('--from', value())
    else if ('--to' === arg) to = named('--to', value())
    else if ('--path' === arg) path = value()
    else if ('--key' === arg) key = value()
    else if ('--with' === arg) program = value()
    else if ('--max-output-bytes' === arg) maxOutputBytes = outputLimit(value())
    else if (arg.startsWith('--')) throw usage()
    else {
      files.push(arg)
      i += 1
      continue
    }
    i += 2
  }
  if (undefined === from || undefined === to || 1 !== files.length) throw usage()
  const input = files[0]
  if ('-' === program && '-' === input) {
    throw new Exit(Fail.input('only one of PROGRAM and INPUT may be - (standard input)'), 2)
  }
  return { from, to, path, key, program, whole, maxOutputBytes, input }
}

// Run the program over one JSON document, into the sink that writes its
// output. The document's events come from JSON's own transducer part over
// the parser's rule passes, the engine's arrays emptied behind them.
function execute(sink: Sink, metrics: Metrics, input: string, limits: Limits): void {
  const json = translate.format('json')
  if (undefined === json) throw Fail.input('no JSON grammar to read the input')
  const part = json.transducerProgram()
  if (undefined === part) throw Fail.input('the JSON grammar ships no transducer part')
  const head = part.parserSinkInto(sink, limits, metrics)
  try {
    json.passes(input).grammar(json.id).prune().limits(limits).metrics(metrics).run(head)
  } catch (err) {
    throw committed(err, metrics)
  }
}

// Run the program over the rule passes of one document, parsed with
// `grammar`, into the sink that writes its output.
function executePasses(
  grammar: translate.Format,
  sink: ParserSink,
  metrics: Metrics,
  input: string,
  limits: Limits,
): void {
  try {
    grammar.passes(input).limits(limits).metrics(metrics).run(sink)
  } catch (err) {
    throw committed(err, metrics)
  }
}

// A run's failure, marked as after output where bytes had reached standard
// output: a failure the source found (bad JSON after the rows) knows
// nothing of the output, and the writer's count says.
function committed(err: unknown, metrics: Metrics): unknown {
  if (isFail(err) && metrics.output_bytes > 0 && !err.committedOutput) err.committed()
  return err
}

// The text to print for the command line. `run` writes its answer itself
// and answers nothing here.
function command(args: ReadonlyArray<string>): string {
  // The command is judged before any file is read, so an unknown one is
  // the usage error whatever the files hold, and never waits on standard
  // input to say so.
  const [name, ...rest] = args
  switch (name) {
    case 'canon':
    case 'format':
    case 'check':
    case 'explain': {
      if (1 !== rest.length) throw usage()
      const file = rest[0]
      const src = read(file)
      switch (name) {
        case 'canon': {
          const text = canonical(parseFile(src, file))
          return '' === text ? text : text + '\n'
        }
        case 'format':
          return format(parseFile(src, file))
        case 'check':
          compile(src, file, OPTIONS)
          return ''
        default:
          return compile(src, file, OPTIONS).explain()
      }
    }
    case 'run': {
      const options = parseRunOptions(rest)
      const src = read(options.program)
      // With --grammar the program reads the rule passes of INPUT parsed
      // as that format, and is compiled over them, as a transducer part
      // is; without it, the JSON document's events.
      const grammar = options.grammar
      const program = compile(src, options.program, undefined === grammar ? OPTIONS : { ...OPTIONS, input: 'ParserEvents/1' })
      const limits: Limits = { ...Limits.default(), max_output_bytes: options.maxOutputBytes }
      // A renderer the program cannot take is refused before the input is
      // read, so the refusal never waits on standard input or drains a
      // large file to say so.
      const metrics = new Metrics()
      if (undefined === grammar) {
        const sink = program.sink(new FdWriter(1), options.render, limits, metrics)
        const input = read(options.input)
        execute(sink, metrics, input, limits)
      } else {
        const sink = program.parserSink(new FdWriter(1), options.render, limits, metrics)
        const input = read(options.input)
        executePasses(grammar, sink, metrics, input, limits)
      }
      return ''
    }
    case 'translate': {
      const options = parseTranslateOptions(rest)
      const limits: Limits = { ...Limits.default(), max_output_bytes: options.maxOutputBytes }
      let path: Segment[] | undefined
      if (undefined !== options.path) {
        try {
          path = translate.parsePath(options.path, limits)
        } catch (err) {
          throw isFail(err) ? new Exit(err, 2) : err
        }
      }
      const program =
        undefined === options.program ? undefined : { file: options.program, text: read(options.program) }
      const request: translate.Request = {
        from: options.from,
        to: options.to,
        path,
        options: { key: options.key ?? alchemyTranslate.Options.default().key },
        program,
        whole: options.whole,
        limits,
      }
      // The composition is compiled before the input is read, so a route
      // that cannot be composed never waits on standard input.
      const compiled = translate.compile(request)
      const input = read(options.input)
      translate.runCompiled(request, compiled, input, new FdWriter(1), new Metrics())
      return ''
    }
    case 'formats':
      if (0 !== rest.length) throw usage()
      return translate.formatsJson() + '\n'
    default:
      throw usage()
  }
}

// The status of a defect of this package (anything thrown that is not a
// `Fail`, from whichever copy of alchemy's shared unit: `isFail`): Rust's
// for a panic, so a script never reads one as an input failure.
const DEFECT = 101

// Run one command line and answer its exit status.
export function main(args: ReadonlyArray<string>): number {
  let text: string
  try {
    text = command(args)
  } catch (err) {
    const exit = err instanceof Exit ? err : isFail(err) ? Exit.of(err) : undefined
    if (undefined === exit) {
      try {
        writeAll(2, `alchemy: internal error: ${err instanceof Error ? (err.stack ?? err.message) : String(err)}\n`)
      } catch (_err) {
        // The status still says what happened.
      }
      return DEFECT
    }
    report(exit.fail)
    return exit.status
  }
  try {
    writeAll(1, text)
  } catch (_err) {
    return 3
  }
  return 0
}
