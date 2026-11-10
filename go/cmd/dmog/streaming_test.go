// Copyright (c) 2026 tabnas, MIT License

package main

// streaming_test.go: the streaming gate (rs/tests/streaming_test.rs in
// Rust, ts/test/streaming.test.ts in TypeScript). Every format the formats
// table lists is held to the definition of streaming in tabnas/alchemy's
// docs/streaming-plan.md, section 13 (admin ADR-33):
//
//	A format streams, for the gate, when in every runtime a document of it
//	read from a file and from standard input has written its first byte
//	before its input ends, and the run's peak memory stays flat over ten
//	times the document.
//
// The documents are test/streaming.json's: for each format, a document of
// its items items and one of ten times as many. Each format is held to two
// clauses:
//
//   - written before the end: dmog translate --from F --to json is handed
//     the document of items items in two parts, from standard input and
//     from a named pipe given as INPUT (a FIFO; there is none on Windows,
//     where only standard input is run), and must write to standard output
//     after the first part, which holds nine tenths of the items, before
//     the rest is sent; the run must then end as a run over the whole
//     document does, with the same output;
//   - flat memory: the command's peak resident set over the document of
//     items items and over ten times as many must stay within
//     streamingBound of each other.
//
// A format that fails either clause is registered in notStreaming with the
// clauses it fails and why; the gate holds each entry to failing exactly
// those, so a format that starts streaming fails it until its entry is
// deleted. The formats the maintainer ruled whole are listed apart
// (ruledWhole) and are not run.
//
// The memory is the child's own peak resident set, getrusage's ru_maxrss
// for the command once it has exited (ProcessState.SysUsage), which counts
// what the command held, the Go runtime and the collector's headroom with
// it, and needs nothing of the test process; on Linux the test also reads
// the child's VmHWM while it runs, and stops a run once it is past the
// bound, so a run that holds its document is not run to its end to show
// it. Unix alone has ru_maxrss, so on Windows the memory clause is not
// measured, and the gate says so.

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"hash/fnv"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/tabnas/dmog/go/translate"
)

// failing is the clauses of the definition a format fails.
type failing int

const (
	// failsWritten: it writes nothing before its input ends, from standard
	// input or from a FIFO.
	failsWritten failing = 1 << iota
	// failsFlat: its peak over ten times the document is past streamingBound
	// of its peak over the document.
	failsFlat
	failsBoth = failsWritten | failsFlat
)

func (f failing) String() string {
	switch f {
	case failsWritten:
		return "written"
	case failsFlat:
		return "flat"
	case failsBoth:
		return "both"
	}
	return "none"
}

// ruledWhole is the formats the maintainer ruled whole (admin ADR-33, ruling
// 4: "expr and semver stay whole"): their documents are read whole by
// design, as their whole sentence in the formats table says, and the gate
// does not run them.
var ruledWhole = map[string]string{
	"expr":   "an expression's tree is complete only at its end; admin ADR-33, ruling 4",
	"semver": "a version is made only once its whole text is accepted; admin ADR-33, ruling 4",
}

// goMemory is why a format that streams in Rust is not flat in Go: the
// engine's rule history, which a stream cannot bound in Go, and the lexer's
// intern pool both keep something of every record.
const goMemory = "the Go engine keeps every rule of the parse, since a bounded rule history loses a " +
	"grammar's writes through its links (tabnas/parser#325), and its lexer interns every distinct " +
	"short string (tabnas/parser#324)"

// notStreaming is the formats that do not stream yet in this runtime, each
// with the clauses it fails and why. The gate holds each entry to failing
// exactly those clauses: delete an entry, or a clause of it, once its format
// passes.
var notStreaming = map[string]struct {
	fails failing
	why   string
}{
	"abnf": {failsBoth, "it has no transducer part: a document is read whole as the grammar spec its compiler " +
		"writes, until abnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)"},
	"c": {failsBoth, "it has no transducer part: a translation unit is read whole as the parser's value " +
		"(admin ADR-33, ruling 7; the streaming plan's step 21, whose ruling is pending)"},
	"css": {failsFlat, "css is not in transduce's prune suite, so the engine's tree keeps every rule " +
		"(tabnas/transduce#82); and " + goMemory},
	"csv": {failsFlat, goMemory},
	"ebnf": {failsBoth, "it has no transducer part: a document is read whole as the grammar spec its compiler " +
		"writes, until ebnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)"},
	"feed": {failsFlat, goMemory},
	"gbnf": {failsBoth, "it has no transducer part: a document is read whole as the grammar spec its compiler " +
		"writes, until gbnf's part (admin ADR-33, ruling 4; the streaming plan's step 23)"},
	"ini": {failsFlat, "every section is a member of the document's root map, and pruning empties no map " +
		"(tabnas/transduce#81); and " + goMemory},
	"json":  {failsFlat, goMemory},
	"json5": {failsFlat, goMemory},
	"jsonc": {failsFlat, goMemory},
	"jsonic": {failsBoth, "jsonic's constructor declares no Lex.Window, so the engine reads a document whole " +
		"before its parse begins (tabnas/jsonic#121)"},
	"jsonl": {failsFlat, goMemory},
	"markdown": {failsFlat, "a document's blocks array is no rule's node, so pruning never empties it, and " +
		"each block's node stays (tabnas/transduce#81); and " + goMemory},
	"pgn": {failsFlat, goMemory},
	"proto": {failsBoth, "the check of its nesting that proto's parse makes first counts the whole document, " +
		"and proto's grammar declares no Lex.Window, so a document is read whole first (tabnas/proto#81)"},
	"toml": {failsFlat, "an array of tables is no rule's node, so pruning never empties it, and each table " +
		"stays whole (tabnas/transduce#81); and " + goMemory},
	"xml":  {failsFlat, goMemory},
	"yaml": {failsFlat, goMemory},
	"zon":  {failsFlat, goMemory},
}

// streamingBound is how far the peak resident set over ten times the
// document may pass the peak over the document: 1.5 times it. No format's
// memory is flat in Go yet, so the bound is set from the runtime's own
// variation and from what a held document costs: the same run's peak varies
// by under a tenth (json over 300 items, eight runs one after another and
// nine three at a time: 18.0 MB to 20.6 MB), and a live heap that stays flat
// keeps the resident set within the collector's headroom of it at both
// sizes, since GOGC=100 lets the heap reach twice what is live before a
// collection. A run that holds its document grows with it, measured one run
// at a time with test/streaming.json's documents (300 items, about 315 KB,
// and 3,000): json 18.88 MB and 70.62 MB (3.7 times), csv 177.85 MB and
// 1392.35 MB, xml 27.09 MB and 139.59 MB, and the least, markdown, 17.97 MB
// and 57.94 MB (3.2 times). JSON read whole (--whole) holds 14.40 MB and
// 40.40 MB (2.8 times), which TestADocumentHeldWholeFailsTheBound holds to
// fail the bound.
const streamingBound = 1.5

// streamingTemplate is a format's document in test/streaming.json: head,
// then item once for each index with {i} the index, separator between two
// items, and tail; {text} in any of them is the file's text.
type streamingTemplate struct {
	Head      string `json:"head"`
	Item      string `json:"item"`
	Separator string `json:"separator"`
	Tail      string `json:"tail"`
}

// streamingDocuments is test/streaming.json.
type streamingDocuments struct {
	Items     int                          `json:"items"`
	Text      string                       `json:"text"`
	Documents map[string]streamingTemplate `json:"documents"`
}

func readStreamingDocuments(t *testing.T) streamingDocuments {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", "..", "test", "streaming.json"))
	if err != nil {
		t.Fatal(err)
	}
	var docs streamingDocuments
	if err := json.Unmarshal(data, &docs); err != nil {
		t.Fatal(err)
	}
	for id, d := range docs.Documents {
		fill := func(s string) string { return strings.ReplaceAll(s, "{text}", docs.Text) }
		docs.Documents[id] = streamingTemplate{fill(d.Head), fill(d.Item), fill(d.Separator), fill(d.Tail)}
	}
	return docs
}

// write writes the document of n items, the items from at to before.
func (d streamingTemplate) write(w io.Writer, n, at, before int) error {
	if at == 0 {
		if _, err := io.WriteString(w, d.Head); err != nil {
			return err
		}
	}
	for i := at; i < before && i < n; i++ {
		if i > 0 {
			if _, err := io.WriteString(w, d.Separator); err != nil {
				return err
			}
		}
		if _, err := io.WriteString(w, strings.ReplaceAll(d.Item, "{i}", fmt.Sprint(i))); err != nil {
			return err
		}
	}
	if before >= n {
		_, err := io.WriteString(w, d.Tail)
		return err
	}
	return nil
}

// file writes the document of n items to a file of its own.
func (d streamingTemplate) file(t *testing.T, dir, id string, n int) string {
	t.Helper()
	path := filepath.Join(dir, fmt.Sprintf("%s-%d", id, n))
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	w := bufio.NewWriter(f)
	if err := d.write(w, n, 0, n); err != nil {
		t.Fatal(err)
	}
	if err := w.Flush(); err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
	return path
}

// memoryRuns lets one measured run go at a time: a command's peak resident
// set takes in the collector's headroom, which a busy machine widens, and
// the formats' other runs go on in parallel.
var memoryRuns sync.Mutex

// written is what a run wrote: its length and a hash of its bytes.
type written struct {
	length int64
	hash   uint64
}

// streamRun is one run of the binary over a file.
type streamRun struct {
	// peak is the command's peak resident set, in bytes, or with over, what
	// it had reached when it was stopped past its limit. measured is false
	// where the platform cannot measure it.
	peak     int64
	over     bool
	measured bool
	output   written
	took     time.Duration
}

// runOverFile runs dmog translate --from id --to json over path, stopped
// past limit bytes of resident set where the platform can watch it (limit
// 0 for none).
func runOverFile(t *testing.T, id, path string, limit int64) streamRun {
	t.Helper()
	memoryRuns.Lock()
	defer memoryRuns.Unlock()
	cmd := exec.Command(binary, "translate", "--from", id, "--to", "json", path)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr
	started := time.Now()
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	stopped := make(chan int64, 1)
	done := make(chan struct{})
	go func() { stopped <- watchPeak(cmd.Process.Pid, limit, done, cmd.Process.Kill) }()
	h := fnv.New64a()
	length, copyErr := io.Copy(h, stdout)
	waitErr := cmd.Wait()
	close(done)
	seen := <-stopped
	over := seen > 0
	took := time.Since(started)
	if !over {
		if copyErr != nil {
			t.Fatalf("%s %s: %v", id, path, copyErr)
		}
		if waitErr != nil {
			t.Fatalf("%s %s: %v: %s", id, path, waitErr, stderr.String())
		}
	}
	peak, measured := peakOf(cmd.ProcessState)
	return streamRun{peak: max(peak, seen), over: over, measured: measured, output: written{length, h.Sum64()}, took: took}
}

// fed is what one run of the binary over a document handed over in two
// parts did.
type fed struct {
	beforeTheEnd bool
	waited       time.Duration
	status       int
	output       written
	stderr       string
}

// feedInTwo runs dmog translate --from id --to json over a document handed
// over in two parts, from standard input (fifo "") or from the FIFO fifo:
// first, then, once the command has written or wait has passed, rest, and
// the end of the input. Standard output is read throughout, so a command
// that writes as it reads never waits on a pipe nobody reads.
func feedInTwo(t *testing.T, id, fifo string, first, rest []byte, wait time.Duration) fed {
	t.Helper()
	input := "-"
	if fifo != "" {
		input = fifo
	}
	cmd := exec.Command(binary, "translate", "--from", id, "--to", "json", input)
	var stdin io.WriteCloser
	var err error
	if fifo == "" {
		if stdin, err = cmd.StdinPipe(); err != nil {
			t.Fatal(err)
		}
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr
	started := time.Now()
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	firstByte := make(chan struct{})
	read := make(chan written, 1)
	go func() {
		h := fnv.New64a()
		var length int64
		buf := make([]byte, 64*1024)
		told := false
		for {
			n, err := stdout.Read(buf)
			if n > 0 {
				h.Write(buf[:n])
				length += int64(n)
				if !told {
					close(firstByte)
					told = true
				}
			}
			if err != nil {
				break
			}
		}
		read <- written{length, h.Sum64()}
	}()
	goOn := make(chan struct{})
	opened := make(chan struct{})
	wrote := make(chan error, 1)
	go func() {
		sink := stdin
		if fifo != "" {
			// A FIFO's open waits for the command to open it to read.
			f, err := os.OpenFile(fifo, os.O_WRONLY, 0)
			if err != nil {
				close(opened)
				wrote <- err
				return
			}
			sink = f
		}
		close(opened)
		defer sink.Close()
		if _, err := sink.Write(first); err != nil {
			wrote <- err
			return
		}
		<-goOn
		_, err := sink.Write(rest)
		wrote <- err
	}()
	before := false
	select {
	case <-firstByte:
		before = true
	case <-time.After(wait):
	}
	waited := time.Since(started)
	close(goOn)
	// Standard output is read to its end before Wait, which closes the pipe
	// once the command has exited.
	var output written
	select {
	case output = <-read:
	case <-time.After(5*time.Minute + 4*wait):
		cmd.Process.Kill()
		output = <-read
	}
	waitErr := cmd.Wait()
	// A command that ended before it opened the FIFO leaves the writer
	// waiting in its open: opening the FIFO to read lets it through, and its
	// write then fails, as one to a command that has ended does.
	for released := false; !released; {
		select {
		case <-opened:
			released = true
		case <-time.After(10 * time.Millisecond):
			releaseFIFO(fifo)
		}
	}
	<-wrote
	status := 0
	var exit *exec.ExitError
	switch {
	case errors.As(waitErr, &exit):
		status = exit.ExitCode()
	case waitErr != nil:
		status = -1
	}
	return fed{beforeTheEnd: before, waited: waited, status: status, output: output, stderr: stderr.String()}
}

// streamFound is what the gate found of one format.
type streamFound struct {
	routes []string
	// when is how long after it started each route's command first wrote,
	// or zero where it wrote nothing before the end.
	when     []time.Duration
	one, ten streamRun
	broken   []string
}

func (f streamFound) fails() failing {
	var fails failing
	for _, when := range f.when {
		if when == 0 {
			fails |= failsWritten
		}
	}
	if f.one.measured && (f.ten.over || float64(f.ten.peak) > float64(f.one.peak)*streamingBound) {
		fails |= failsFlat
	}
	return fails
}

func mib(bytes int64) string { return fmt.Sprintf("%.2f MB", float64(bytes)/(1<<20)) }

func (f streamFound) summary(items int) string {
	var parts []string
	for i, route := range f.routes {
		if f.when[i] > 0 {
			parts = append(parts, fmt.Sprintf("from %s written after %.1f s", route, f.when[i].Seconds()))
		} else {
			parts = append(parts, fmt.Sprintf("from %s nothing written before the end", route))
		}
	}
	memory := "memory not measured on " + runtime.GOOS
	if f.one.measured {
		stopped := ""
		if f.ten.over {
			stopped = ", stopped past it"
		}
		memory = fmt.Sprintf("peak %s at %d items and %s at %d (bound %s%s), %.1f s and %.1f s",
			mib(f.one.peak), items, mib(f.ten.peak), items*10,
			mib(int64(float64(f.one.peak)*streamingBound)), stopped, f.one.took.Seconds(), f.ten.took.Seconds())
	}
	return strings.Join(parts, ", ") + "; " + memory
}

// holdStreaming holds one format to both clauses.
func holdStreaming(t *testing.T, docs streamingDocuments, format *translate.Format) streamFound {
	id := format.ID()
	d := docs.Documents[id]
	n := docs.Items
	dir := t.TempDir()
	one := runOverFile(t, id, d.file(t, dir, id, n), 0)
	entry, registered := notStreaming[id]
	// A command expected to write before the end is given a generous time
	// to; one registered as not writing is given twice the time a whole run
	// took, and two seconds, which a run that wrote as it read would not
	// need.
	wait := max(60*time.Second, 10*one.took)
	if registered && entry.fails&failsWritten != 0 {
		wait = 2*one.took + 2*time.Second
	}
	var first, rest strings.Builder
	split := n * 9 / 10
	d.write(&first, n, 0, split)
	d.write(&rest, n, split, n)
	found := streamFound{one: one}
	routes := []string{"standard input"}
	fifos := []string{""}
	if fifo := makeFIFO(t, dir, id); fifo != "" {
		routes = append(routes, "a FIFO")
		fifos = append(fifos, fifo)
	}
	feeds := make([]chan fed, len(fifos))
	for i, fifo := range fifos {
		feeds[i] = make(chan fed, 1)
		go func(i int, fifo string) {
			feeds[i] <- feedInTwo(t, id, fifo, []byte(first.String()), []byte(rest.String()), wait)
		}(i, fifo)
	}
	for i, route := range routes {
		f := <-feeds[i]
		switch {
		case f.status != 0:
			found.broken = append(found.broken, fmt.Sprintf("from %s: status %d: %s", route, f.status, strings.TrimSpace(f.stderr)))
		case f.output != one.output:
			found.broken = append(found.broken, fmt.Sprintf("from %s: %d bytes written, where the run over the whole "+
				"document wrote %d, or other bytes", route, f.output.length, one.output.length))
		}
		found.routes = append(found.routes, route)
		if f.beforeTheEnd {
			found.when = append(found.when, f.waited)
		} else {
			found.when = append(found.when, 0)
		}
	}
	var limit int64
	if one.measured {
		limit = int64(float64(one.peak) * streamingBound)
	}
	found.ten = runOverFile(t, id, d.file(t, dir, id, n*10), limit)
	return found
}

// TestEveryFormatStreamsOrIsRegistered holds every format the formats table
// lists to the definition: it streams, or it is registered in notStreaming,
// failing exactly the clauses its entry names, or in ruledWhole. The
// formats run in parallel, each its own commands.
func TestEveryFormatStreamsOrIsRegistered(t *testing.T) {
	docs := readStreamingDocuments(t)
	for _, format := range translate.Formats() {
		id := format.ID()
		if _, whole := ruledWhole[id]; whole {
			continue
		}
		if _, ok := docs.Documents[id]; !ok {
			t.Errorf("%s is a format with no document in test/streaming.json", id)
			continue
		}
		t.Run(id, func(t *testing.T) {
			t.Parallel()
			found := holdStreaming(t, docs, format)
			summary := found.summary(docs.Items)
			t.Logf("%s: %s", id, summary)
			for _, broken := range found.broken {
				t.Errorf("%s %s", id, broken)
			}
			fails := found.fails()
			entry, registered := notStreaming[id]
			if !found.one.measured {
				// The memory clause is not measured here: hold what was.
				fails &^= failsFlat
				entry.fails &^= failsFlat
			}
			switch {
			case !registered && fails == 0:
			case registered && fails == entry.fails:
			case fails == 0:
				t.Errorf("%s streams now (%s): delete its entry, %s (%s)", id, summary, entry.fails, entry.why)
			case !registered:
				t.Errorf("%s does not stream: it fails %s (%s); register it with why", id, fails, summary)
			default:
				t.Errorf("%s fails %s, where its entry says %s (%s): correct the entry (%s)", id, fails, entry.fails, summary, entry.why)
			}
		})
	}
}

// TestADocumentHeldWholeFailsTheBound holds the bound to fail a document
// read whole: JSON read with --whole, its grammar's value walked, holds
// more over ten times the document than the bound lets a run hold.
func TestADocumentHeldWholeFailsTheBound(t *testing.T) {
	docs := readStreamingDocuments(t)
	d, n, dir := docs.Documents["json"], docs.Items, t.TempDir()
	whole := func(path string, limit int64) streamRun {
		// runOverFile's command, with --whole.
		cmd := exec.Command(binary, "translate", "--from", "json", "--to", "json", "--whole", path)
		cmd.Stdout = io.Discard
		if err := cmd.Start(); err != nil {
			t.Fatal(err)
		}
		stopped := make(chan int64, 1)
		done := make(chan struct{})
		go func() { stopped <- watchPeak(cmd.Process.Pid, limit, done, cmd.Process.Kill) }()
		err := cmd.Wait()
		close(done)
		seen := <-stopped
		if err != nil && seen == 0 {
			t.Fatalf("json --whole %s: %v", path, err)
		}
		peak, measured := peakOf(cmd.ProcessState)
		return streamRun{peak: max(peak, seen), over: seen > 0, measured: measured}
	}
	one := whole(d.file(t, dir, "json", n), 0)
	if !one.measured {
		t.Skip("memory is not measured on " + runtime.GOOS)
	}
	limit := int64(float64(one.peak) * streamingBound)
	ten := whole(d.file(t, dir, "json", n*10), limit)
	t.Logf("json read whole: peak %s at %d items and %s at %d", mib(one.peak), n, mib(ten.peak), n*10)
	if !ten.over && ten.peak <= limit {
		t.Errorf("json read whole held %s at %d items and %s at %d, within the bound %s",
			mib(one.peak), n, mib(ten.peak), n*10, mib(limit))
	}
}

// TestTheStreamingLedgersAndDocumentsAreTheTables holds the ledgers and the
// documents to the formats table: each entry a format that is run, each
// ruling a format the table reads whole, and each document a format the
// gate runs.
func TestTheStreamingLedgersAndDocumentsAreTheTables(t *testing.T) {
	docs := readStreamingDocuments(t)
	for id, entry := range notStreaming {
		if translate.Named(id) == nil {
			t.Errorf("%s (%s) is no format", id, entry.why)
		} else if _, whole := ruledWhole[id]; whole {
			t.Errorf("%s is registered and ruled whole", id)
		}
	}
	for id, why := range ruledWhole {
		f := translate.Named(id)
		if f == nil {
			t.Errorf("%s (%s) is no format", id, why)
		} else if f.Part.Whole == "" || f.Transducer != nil {
			t.Errorf("%s is ruled whole (%s), but the formats table no longer reads it whole", id, why)
		}
	}
	for id := range docs.Documents {
		if _, whole := ruledWhole[id]; translate.Named(id) == nil || whole {
			t.Errorf("test/streaming.json has a document for %s, which the gate does not run", id)
		}
	}
}
