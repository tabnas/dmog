// Copyright (c) 2026 tabnas, MIT License

// Command dmog is the alchemy language's command line, the Go port of
// rs/src/bin/dmog.rs:
//
//	dmog canon FILE       print the program in canonical form
//	dmog format FILE      print the program in layout form
//	dmog check FILE       parse, desugar, resolve, check and build the plan; print nothing and exit 0
//	dmog explain FILE     print the plan report
//	dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
//	                      run the program over the JSON document INPUT; with --grammar, over
//	                      the rule passes of INPUT parsed as FORMAT (a transducer part's input)
//	dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
//	               [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT
//	                      write INPUT, read as FORMAT, in another format
//	dmog formats          print the formats translate reads and writes, as JSON
//
// FILE, PROGRAM and INPUT may be - for standard input (one of them per
// run). Nothing but the answer goes to standard output. A failure is the
// Fail as one JSON object on standard error (code, message, and path,
// limit, row, col when they apply, and output: "partial" when bytes had
// reached standard output). The exit status follows the code: 2 for a
// program that does not read, resolve or check (DSL_PARSE_ERROR,
// DSL_TYPE_ERROR, STREAM_REUSED, STREAMABILITY_UNKNOWN), for a usage error
// and for an unreadable file; 1 for an input or protocol failure (from
// check too, when building the plan reaches a fail); 5 for
// RESOURCE_LIMIT_EXCEEDED; 3 for OUTPUT_FAILED; 6 for ABORTED. A standard
// error that cannot be written loses the report, not the status.
//
// --max-output-bytes sets the writer's limit; the others are transduce's
// defaults.
//
// run reads INPUT as JSON through JSON's own transducer part, the one
// translate reads a JSON document through: the part makes the document's
// events of the parser's rule passes, and the engine's arrays are emptied
// behind them. A document of another format is `translate --with`'s
// business: it runs the program over INPUT read as --from. The output goes
// through a coalescing writer to standard output and is flushed once, at
// the end; a failure found after bytes were written says so. With
// --grammar FORMAT, run parses INPUT with that format's grammar through
// transduce's ParserEventSource and compiles the program over the parser's
// rule passes, as a transducer part is compiled. The source of rule passes
// needs no adapter of transduce's, so every run reads its document in every
// build, with the tabnas_nodecell tag or without it.
//
// translate reads INPUT with the grammar of --from and writes it in --to,
// through the translation parts each format's module exports, as the
// translate package composes them: any of the formats `dmog formats`
// lists into any other. --path takes a JSON array of keys and indexes
// (["people",0]) and translates that value instead of the document; --key
// names the member a root is wrapped under for a format whose document must
// be an object (items by default); --with runs a program over the input
// first, as run does, and writes its export's events or table in --to.
package main

import (
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"unicode"
	"unicode/utf8"

	alchemy "github.com/tabnas/alchemy/go"
	at "github.com/tabnas/alchemy/go/translate"
	"github.com/tabnas/dmog/go/translate"
	tr "github.com/tabnas/render/go"
	tt "github.com/tabnas/transduce/go"
)

// VERSION is this module's version. It MUST equal ts/package.json
// "version": admin/publish.sh rewrites both, and
// TestVersionMatchesPackageJSON fails the build if they drift. Keep it the
// only `^const VERSION =` in the module, which is how the orchestrator
// finds it.
const VERSION = "0.2.0"

const usage = "usage: dmog canon|format|check|explain FILE\n       dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT\n       dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY] [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT\n       dmog formats\n       (a FILE may be - for standard input)"

func main() {
	// A write to a closed standard output or standard error fails with an
	// error the command reports (OUTPUT_FAILED, status 3) or drops (the
	// report on standard error), as the Rust binary's does, rather than
	// ending the process on SIGPIPE, which Go does for those two
	// descriptors unless the signal is ignored.
	signal.Ignore(syscall.SIGPIPE)
	os.Exit(command(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

// command runs one command line and answers its exit status.
func command(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	text, exit := run(args, stdin, stdout)
	if exit != nil {
		report(stderr, exit.fail)
		return exit.status
	}
	if _, err := io.WriteString(stdout, text); err != nil {
		return 3
	}
	return 0
}

// report writes the failure as one JSON object on standard error. A
// standard error that cannot be written (a closed pipe) loses the report,
// not the status.
func report(stderr io.Writer, f *tt.Fail) {
	data, err := f.MarshalJSON()
	if err != nil {
		return
	}
	_, _ = stderr.Write(append(data, '\n'))
}

// exit is a failure and the status it exits with: the code's, except that
// a usage error and an unreadable file exit 2 although their code is
// INPUT_INVALID, since they are the command line's fault, not the
// document's.
type exit struct {
	fail   *tt.Fail
	status int
}

func fromFail(f *tt.Fail) *exit {
	status := 1
	switch f.Code {
	case tt.CodeDSLParseError, tt.CodeDSLTypeError, tt.CodeStreamReused, tt.CodeStreamabilityUnknown:
		status = 2
	case tt.CodeResourceLimitExceeded:
		status = 5
	case tt.CodeOutputFailed:
		status = 3
	case tt.CodeAborted:
		status = 6
	}
	return &exit{fail: f, status: status}
}

func usageError() *exit { return &exit{fail: tt.InputFail(usage), status: 2} }

// read is a file's text, or standard input's for "-".
func read(file string, stdin io.Reader) (string, *exit) {
	var data []byte
	var err error
	if file == "-" {
		data, err = io.ReadAll(stdin)
	} else {
		data, err = os.ReadFile(file)
	}
	if err != nil {
		return "", &exit{fail: tt.InputFail(fmt.Sprintf("cannot read %s: %s", file, ioMessage(err))), status: 2}
	}
	// A Go string holds any bytes, so malformed UTF-8 is refused here, as
	// the Rust binary's read_to_string and the TypeScript bin's fatal
	// decoder refuse it while reading, with Rust's message.
	if !utf8.Valid(data) {
		return "", &exit{fail: tt.InputFail(fmt.Sprintf("cannot read %s: stream did not contain valid UTF-8", file)), status: 2}
	}
	return string(data), nil
}

// ioMessage is an I/O error as the Rust binary's message has it, from the
// operating system's own text and number: `No such file or directory (os
// error 2)`.
func ioMessage(err error) string {
	var pathError *fs.PathError
	if errors.As(err, &pathError) {
		err = pathError.Err
	}
	var errno syscall.Errno
	if errors.As(err, &errno) {
		text := errno.Error()
		if text != "" {
			text = strings.ToUpper(text[:1]) + text[1:]
		}
		return fmt.Sprintf("%s (os error %d)", text, int(errno))
	}
	return err.Error()
}

// run is the text to print for the command line, or the failure. `run`
// writes its answer itself and answers nothing here.
func run(args []string, stdin io.Reader, stdout io.Writer) (string, *exit) {
	// The command is judged before any file is read, so an unknown one is
	// the usage error whatever the files hold, and never waits on standard
	// input to say so.
	if len(args) == 0 {
		return "", usageError()
	}
	switch command, rest := args[0], args[1:]; command {
	case "canon", "format", "check", "explain":
		if len(rest) != 1 {
			return "", usageError()
		}
		file := rest[0]
		src, e := read(file, stdin)
		if e != nil {
			return "", e
		}
		switch command {
		case "canon":
			program, f := alchemy.ParseFile(src, file)
			if f != nil {
				return "", fromFail(f)
			}
			text := alchemy.Canonical(program)
			if text != "" {
				text += "\n"
			}
			return text, nil
		case "format":
			program, f := alchemy.ParseFile(src, file)
			if f != nil {
				return "", fromFail(f)
			}
			return alchemy.Format(program), nil
		case "check":
			if _, f := alchemy.Compile(src, file, tt.Routers(), tr.Renderers()); f != nil {
				return "", fromFail(f)
			}
			return "", nil
		}
		program, f := alchemy.Compile(src, file, tt.Routers(), tr.Renderers())
		if f != nil {
			return "", fromFail(f)
		}
		return program.Explain(), nil
	case "run":
		options, e := parseRunOptions(rest)
		if e != nil {
			return "", e
		}
		src, e := read(options.program, stdin)
		if e != nil {
			return "", e
		}
		// With --grammar the program reads the rule passes of INPUT parsed
		// as that format, and is compiled over them, as a transducer part
		// is; without it, the JSON document's events.
		var program *alchemy.Program
		var f *alchemy.Fail
		if options.grammar != nil {
			program, f = alchemy.CompileWith(src, options.program, alchemy.InputParserEvents, tt.Routers(), tr.Renderers())
		} else {
			program, f = alchemy.Compile(src, options.program, tt.Routers(), tr.Renderers())
		}
		if f != nil {
			return "", fromFail(f)
		}
		limits := tt.DefaultLimits()
		limits.MaxOutputBytes = options.maxOutputBytes
		// Whatever refuses the run is found before the input is read, so a
		// refusal never waits on standard input or drains a large file to
		// say so: the renderer the program cannot take.
		metrics := tt.NewMetrics()
		if options.grammar != nil {
			sink, f := program.ParserSink(stdout, options.render, limits, metrics)
			if f != nil {
				return "", fromFail(f)
			}
			input, e := read(options.input, stdin)
			if e != nil {
				return "", e
			}
			if f := executePasses(options.grammar, sink, metrics, input, limits); f != nil {
				return "", fromFail(f)
			}
			return "", nil
		}
		sink, f := program.Sink(stdout, options.render, limits, metrics)
		if f != nil {
			return "", fromFail(f)
		}
		input, e := read(options.input, stdin)
		if e != nil {
			return "", e
		}
		if f := execute(sink, metrics, input, limits); f != nil {
			return "", fromFail(f)
		}
		return "", nil
	case "translate":
		options, e := parseTranslateOptions(rest)
		if e != nil {
			return "", e
		}
		limits := tt.DefaultLimits()
		limits.MaxOutputBytes = options.maxOutputBytes
		request := &translate.Request{
			From:    options.from,
			To:      options.to,
			Options: at.DefaultOptions(),
			Whole:   options.whole,
			Limits:  limits,
		}
		if options.key != nil {
			request.Options.Key = *options.key
		}
		if options.path != nil {
			path, f := translate.ParsePath(*options.path, limits)
			if f != nil {
				return "", &exit{fail: f, status: 2}
			}
			request.Path = path
		}
		if options.program != nil {
			src, e := read(*options.program, stdin)
			if e != nil {
				return "", e
			}
			request.Program = &alchemy.Source{File: *options.program, Text: src}
		}
		// The composition is compiled before the input is read, so a route
		// that cannot be composed never waits on standard input.
		compiled, f := translate.Compile(request)
		if f != nil {
			return "", fromFail(f)
		}
		input, e := read(options.input, stdin)
		if e != nil {
			return "", e
		}
		if f := translate.RunCompiled(request, compiled, input, rustIO{stdout}, tt.NewMetrics()); f != nil {
			return "", fromFail(f)
		}
		return "", nil
	case "formats":
		if len(rest) != 0 {
			return "", usageError()
		}
		return translate.FormatsJSON() + "\n", nil
	}
	return "", usageError()
}

// rustIO is a writer whose failures read as the Rust binary's I/O errors
// (ioMessage), for the message that quotes one: `Broken pipe (os error
// 32)`.
type rustIO struct{ w io.Writer }

func (r rustIO) Write(p []byte) (int, error) {
	n, err := r.w.Write(p)
	if err != nil {
		return n, errors.New(ioMessage(err))
	}
	return n, nil
}

// debugQuote is s as Rust's Debug writes a str, as the Rust binary's
// messages quote an argument: in double quotes, with \0, \t, \r, \n, \\
// and \" escaped, a character that extends a grapheme or is not printable
// as \u{...} in lowercase hex, and every other character as itself.
func debugQuote(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case 0:
			b.WriteString(`\0`)
		case '\t':
			b.WriteString(`\t`)
		case '\r':
			b.WriteString(`\r`)
		case '\n':
			b.WriteString(`\n`)
		case '\\':
			b.WriteString(`\\`)
		case '"':
			b.WriteString(`\"`)
		default:
			if unicode.In(r, unicode.Mn, unicode.Me, unicode.Other_Grapheme_Extend) || !unicode.IsPrint(r) {
				fmt.Fprintf(&b, `\u{%x}`, r)
			} else {
				b.WriteRune(r)
			}
		}
	}
	b.WriteByte('"')
	return b.String()
}

type translateOptions struct {
	from, to       *translate.Format
	path           *string
	key            *string
	program        *string
	whole          bool
	maxOutputBytes *uint64
	input          string
}

// parseTranslateOptions reads translate's command line as the Rust
// binary's does: in order, each flag taking the argument after it whatever
// that is, a later flag replacing an earlier one, and the format --from or
// --to names looked up where it stands.
func parseTranslateOptions(args []string) (*translateOptions, *exit) {
	options := &translateOptions{}
	named := func(flag, id string) (*translate.Format, *exit) {
		format := translate.Named(id)
		if format == nil {
			return nil, &exit{fail: tt.InputFail(flag + " takes " + translate.Names() + ", not " + debugQuote(id)), status: 2}
		}
		return format, nil
	}
	var files []string
	for i := 0; i < len(args); {
		arg := args[i]
		if arg == "--whole" {
			options.whole = true
			i++
			continue
		}
		switch arg {
		case "--from", "--to", "--path", "--key", "--with", "--max-output-bytes":
		default:
			if strings.HasPrefix(arg, "--") {
				return nil, usageError()
			}
			files = append(files, arg)
			i++
			continue
		}
		if i+1 >= len(args) {
			return nil, usageError()
		}
		value := args[i+1]
		switch arg {
		case "--from":
			format, e := named(arg, value)
			if e != nil {
				return nil, e
			}
			options.from = format
		case "--to":
			format, e := named(arg, value)
			if e != nil {
				return nil, e
			}
			options.to = format
		case "--path":
			options.path = &value
		case "--key":
			options.key = &value
		case "--with":
			options.program = &value
		case "--max-output-bytes":
			n, ok := parseU64(value)
			if !ok {
				return nil, &exit{fail: tt.InputFail("--max-output-bytes takes a number of bytes, not " + debugQuote(value)), status: 2}
			}
			options.maxOutputBytes = &n
		}
		i += 2
	}
	if options.from == nil || options.to == nil || len(files) != 1 {
		return nil, usageError()
	}
	options.input = files[0]
	if options.program != nil && *options.program == "-" && options.input == "-" {
		return nil, &exit{fail: tt.InputFail("only one of PROGRAM and INPUT may be - (standard input)"), status: 2}
	}
	return options, nil
}

type runOptions struct {
	// grammar is the format whose parser reads INPUT, its rule passes the
	// program's input (--grammar); nil for the JSON document's events.
	grammar        *translate.Format
	render         alchemy.Renderer
	maxOutputBytes *uint64
	program        string
	input          string
}

// parseU64 reads a byte count as Rust's u64 parse does: decimal digits,
// with an optional leading +.
func parseU64(text string) (uint64, bool) {
	digits := strings.TrimPrefix(text, "+")
	if digits == "" || strings.ContainsAny(digits[:1], "+-") {
		return 0, false
	}
	n, err := strconv.ParseUint(digits, 10, 64)
	return n, err == nil
}

func parseRunOptions(args []string) (*runOptions, *exit) {
	options := &runOptions{}
	var files []string
	for i := 0; i < len(args); {
		switch arg := args[i]; {
		case arg == "--grammar":
			if i+1 >= len(args) {
				return nil, usageError()
			}
			name := args[i+1]
			grammar := translate.Named(name)
			if grammar == nil || !grammar.Parses() {
				return nil, &exit{fail: tt.InputFail("--grammar takes a format whose documents are parsed (" + translate.ParserNames() + "), not " + strconv.Quote(name)), status: 2}
			}
			options.grammar = grammar
			i += 2
		case arg == "--render":
			if i+1 >= len(args) {
				return nil, usageError()
			}
			name := args[i+1]
			render, ok := alchemy.RendererNamed(name)
			if !ok {
				return nil, &exit{fail: tt.InputFail("--render takes csv or json, not " + strconv.Quote(name)), status: 2}
			}
			options.render = render
			i += 2
		case arg == "--max-output-bytes":
			if i+1 >= len(args) {
				return nil, usageError()
			}
			n, ok := parseU64(args[i+1])
			if !ok {
				return nil, &exit{fail: tt.InputFail("--max-output-bytes takes a number of bytes, not " + strconv.Quote(args[i+1])), status: 2}
			}
			options.maxOutputBytes = &n
			i += 2
		case strings.HasPrefix(arg, "--"):
			return nil, usageError()
		default:
			files = append(files, arg)
			i++
		}
	}
	if len(files) != 2 {
		return nil, usageError()
	}
	if files[0] == "-" && files[1] == "-" {
		return nil, &exit{fail: tt.InputFail("only one of PROGRAM and INPUT may be - (standard input)"), status: 2}
	}
	options.program, options.input = files[0], files[1]
	return options, nil
}

// execute runs the program over one JSON document, into the sink that
// writes its output. The document's events come from JSON's own transducer
// part over the parser's rule passes, the engine's arrays emptied behind
// them.
func execute(sink tt.Sink, metrics *tt.Metrics, input string, limits tt.Limits) *tt.Fail {
	json := translate.Named("json")
	if json == nil {
		return tt.InputFail("no JSON grammar to read the input")
	}
	part, f := json.TransducerProgram()
	if f != nil {
		return f
	}
	if part == nil {
		return tt.InputFail("the JSON grammar ships no transducer part")
	}
	head, f := part.ParserSinkInto(sink, limits, metrics)
	if f != nil {
		return f
	}
	source, f := json.Passes(input)
	if f != nil {
		return committed(f, metrics)
	}
	_, f = source.Grammar(json.ID()).Prune().Limits(limits).Metrics(metrics).Run(head)
	return committed(f, metrics)
}

// executePasses runs the program over the rule passes of one document,
// parsed with grammar, into the sink that writes its output.
func executePasses(grammar *translate.Format, sink tt.ParserSink, metrics *tt.Metrics, input string, limits tt.Limits) *tt.Fail {
	source, f := grammar.Passes(input)
	if f != nil {
		return committed(f, metrics)
	}
	_, f = source.Limits(limits).Metrics(metrics).Run(sink)
	return committed(f, metrics)
}

// committed is a run's failure, marked as after output where bytes had
// reached standard output: a failure the source found (bad JSON after the
// rows) knows nothing of the output, and the writer's count says.
func committed(f *tt.Fail, metrics *tt.Metrics) *tt.Fail {
	if f != nil && metrics.OutputBytes.Load() > 0 && !f.CommittedOutput {
		f.Committed()
	}
	return f
}
