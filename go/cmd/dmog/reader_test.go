// Copyright (c) 2026 tabnas, MIT License

package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tabnasjson "github.com/tabnas/json/go"
)

// run and translate read INPUT through a reader, as the Rust binary does: a
// file opened, or standard input, read once. Where the source's grammar
// declares Lex.Window the engine reads it a window at a time, so the output
// streams before the input ends; JSON's does from the release of
// github.com/tabnas/json/go that declares it, and before that release the
// engine reads the reader to its end first. Each test is held to the module
// this build has, whichever side of that release.

// streamRecords is a long JSON array's elements, a little over half a
// megabyte of them.
func streamRecords() string {
	records := make([]string, 20000)
	for i := range records {
		records[i] = fmt.Sprintf(`{"id":%d,"name":"n%d"}`, i, i)
	}
	return strings.Join(records, ",")
}

// streamCommands are the commands that read INPUT through a reader: a
// program over a JSON document, and a translation from JSON.
func streamCommands(t *testing.T) [][]string {
	echo := tempFile(t, "stream-echo.alc", "def export [input] input\n")
	return [][]string{{"run", echo}, {"translate", "--from", "json", "--to", "json"}}
}

// Bytes that are not UTF-8 near the end of a long document fail the run as
// a file that cannot be read, from a file and from standard input alike:
// after the output of the document before them, which the failure calls
// partial, where the grammar reads a window at a time, and before any output
// where the engine reads the whole document first.
func TestALongDocumentWithBytesThatAreNotUTF8NearItsEndCannotBeRead(t *testing.T) {
	windowed := tabnasjson.Make().Config().Window
	records := streamRecords()
	good := tempFile(t, "stream-good.json", "["+records+"]")
	bytes := "[" + records + `,"` + "\xff" + `"]`
	bad := tempFile(t, "stream-bad.json", bytes)
	for _, command := range streamCommands(t) {
		whole := invoke(t, append(append([]string{}, command...), good), nil)
		if whole.status != 0 {
			t.Fatalf("%s: %+v", command[0], whole.stderr)
		}
		for _, c := range []struct {
			input string
			stdin *string
		}{{bad, nil}, {"-", text(bytes)}} {
			at := command[0] + " " + c.input
			o := invoke(t, append(append([]string{}, command...), c.input), c.stdin)
			f := failJSON(t, o)
			if o.status != 2 || f["code"] != "INPUT_INVALID" || f["message"] != "cannot read "+c.input+": stream did not contain valid UTF-8" {
				t.Errorf("%s: status %d, failure %v", at, o.status, f)
				continue
			}
			if windowed {
				if f["output"] != "partial" || len(o.stdout) < 32*1024 || !strings.HasPrefix(whole.stdout, o.stdout) {
					t.Errorf("%s: output %v, %d bytes written", at, f["output"], len(o.stdout))
				}
			} else if f["output"] != "none" || o.stdout != "" {
				t.Errorf("%s: output %v, %d bytes written", at, f["output"], len(o.stdout))
			}
		}
	}
}

// writtenBeforeTheEnd runs args over standard input held open after first,
// which it must write from within 60 s; then rest follows, and the run must
// end with status 0. It is everything the run wrote.
func writtenBeforeTheEnd(t *testing.T, args []string, first, rest string) string {
	t.Helper()
	cmd := exec.Command(binary, append(append([]string{}, args...), "-")...)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	var written strings.Builder
	started := make(chan struct{})
	drained := make(chan error, 1)
	go func() {
		buf := make([]byte, 64*1024)
		for {
			n, err := stdout.Read(buf)
			if n > 0 {
				if written.Len() == 0 {
					close(started)
				}
				written.Write(buf[:n])
			}
			if err != nil {
				if errors.Is(err, io.EOF) {
					err = nil
				}
				drained <- err
				return
			}
		}
	}()
	if _, err := io.WriteString(stdin, first); err != nil {
		t.Fatal(err)
	}
	select {
	case <-started:
	case <-time.After(60 * time.Second):
		cmd.Process.Kill()
		t.Fatalf("%s wrote nothing in 60 s of half a document: %s", strings.Join(args, " "), stderr.String())
	}
	if _, err := io.WriteString(stdin, rest); err != nil {
		t.Fatal(err)
	}
	stdin.Close()
	if err := <-drained; err != nil {
		t.Fatal(err)
	}
	if err := cmd.Wait(); err != nil {
		t.Fatalf("%s: %v: %s", strings.Join(args, " "), err, stderr.String())
	}
	return written.String()
}

// A long document's output starts before its input ends: standard input is
// held open after half the document, and the command has written by then.
// The other half follows, and the run ends as it would have.
func TestALongDocumentIsWrittenBeforeItsInputEnds(t *testing.T) {
	if !tabnasjson.Make().Config().Window {
		t.Skip("github.com/tabnas/json/go declares no Lex.Window in this build")
	}
	records := streamRecords()
	for _, command := range streamCommands(t) {
		writtenBeforeTheEnd(t, command, "["+records, ","+records+"]")
	}
}

// So is a JSON5 document, whose check reads only as far as its leading
// comment (CheckReader), and a ZON document: their modules' constructors
// declare Lex.Window too.
func TestAJSON5OrZONDocumentIsWrittenBeforeItsInputEnds(t *testing.T) {
	each := func(item func(i int) string, separator string) string {
		items := make([]string, 20000)
		for i := range items {
			items[i] = item(i)
		}
		return strings.Join(items, separator)
	}
	json5 := each(func(i int) string { return fmt.Sprintf("{id:%d,name:'n%d'}", i, i) }, ",")
	zon := each(func(i int) string { return fmt.Sprintf(`.{ .id = %d, .name = "n%d" }`, i, i) }, ", ")
	for _, c := range []struct{ from, first, rest string }{
		{"json5", "// records\n[" + json5, "," + json5 + ",]"},
		{"zon", ".{ " + zon, ", " + zon + " }"},
	} {
		writtenBeforeTheEnd(t, []string{"translate", "--from", c.from, "--to", "json"}, c.first, c.rest)
	}
}

// So does the value a path names below the root: a sub-stream of the
// document's events, written as they come, and all of the value.
func TestAPathsValueIsWrittenBeforeItsInputEnds(t *testing.T) {
	if !tabnasjson.Make().Config().Window {
		t.Skip("github.com/tabnas/json/go declares no Lex.Window in this build")
	}
	records := streamRecords()
	got := writtenBeforeTheEnd(t,
		[]string{"translate", "--from", "json", "--to", "jsonl", "--path", `["rows"]`},
		`{"meta":{"n":2},"rows":[`+records, ","+records+`],"after":true}`)
	lines := strings.ReplaceAll(records, "},{", "}\n{")
	if want := lines + "\n" + lines + "\n"; got != want {
		t.Errorf("the path's value: %d bytes, want %d", len(got), len(want))
	}
}

// An input that cannot be opened, and one that fails part way, a
// directory, are refused as files that cannot be read, in the Rust
// binary's words, whether the route reads through a reader or whole.
func TestAnInputThatCannotBeReadIsRefusedInTheRustBinarysWords(t *testing.T) {
	dir := t.TempDir()
	missing := filepath.Join(dir, "missing.json")
	if _, err := os.Stat(missing); err == nil {
		t.Fatal("the missing file exists")
	}
	for _, command := range streamCommands(t) {
		for _, c := range []struct {
			input, reason string
		}{
			{missing, "No such file or directory (os error 2)"},
			{dir, "Is a directory (os error 21)"},
		} {
			at := command[0] + " " + c.input
			o := invoke(t, append(append([]string{}, command...), c.input), nil)
			f := failJSON(t, o)
			if o.status != 2 || f["code"] != "INPUT_INVALID" || f["message"] != "cannot read "+c.input+": "+c.reason || f["output"] != "none" || o.stdout != "" {
				t.Errorf("%s: status %d, failure %v", at, o.status, f)
			}
		}
	}
	// A route that reads the document whole reads it through the same
	// reader, so it says the same.
	o := invoke(t, []string{"translate", "--from", "json", "--to", "json", "--whole", dir}, nil)
	if f := failJSON(t, o); o.status != 2 || f["message"] != "cannot read "+dir+": Is a directory (os error 21)" {
		t.Errorf("--whole: status %d, failure %v", o.status, f)
	}
}
