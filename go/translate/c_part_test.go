package translate_test

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	support "github.com/tabnas/support/go"
	tt "github.com/tabnas/transduce/go"
)

// cOutcome is how a C document read through C's transducer part compares
// with the same document read whole (--whole, the parser's value walked).
type cOutcome int

const (
	// cRead: the same tree, written as the same text.
	cRead cOutcome = iota
	// cReordered: the same tree, its members written in another order.
	cReordered
	// cDeep: refused on max_depth by both routes, a tree past the default
	// limit.
	cDeep
	// cRefused: refused alike, with the same code, message and position.
	cRefused
)

// pastTheDepth is whether f refuses a document nested past max_depth.
func pastTheDepth(f *tt.Fail) bool {
	return f != nil && f.Code == tt.CodeResourceLimitExceeded && f.Limit != nil && f.Limit.Name == "max_depth"
}

// cThroughThePart translates text, a C document, into JSON through C's
// part and read whole: the same tree, compared as a value, or the same
// refusal, its code, message and position. It reports a difference as an
// error naming the document.
func cThroughThePart(t *testing.T, name, text string) (cOutcome, error) {
	c, js := format(t, "c"), format(t, "json")
	part, partFail := translateAs(c, js, text, nil, false)
	whole, wholeFail := translateAs(c, js, text, nil, true)
	switch {
	case partFail == nil && wholeFail == nil:
		if part == whole {
			return cRead, nil
		}
		var p, w any
		if err := json.Unmarshal([]byte(part), &p); err != nil {
			return 0, fmt.Errorf("%s: the part's JSON: %v", name, err)
		}
		if err := json.Unmarshal([]byte(whole), &w); err != nil {
			return 0, fmt.Errorf("%s: the reader's JSON: %v", name, err)
		}
		if !reflect.DeepEqual(p, w) {
			return 0, fmt.Errorf("%s: the part's tree is not the reader's", name)
		}
		return cReordered, nil
	case partFail != nil && wholeFail != nil:
		if pastTheDepth(partFail) && pastTheDepth(wholeFail) {
			return cDeep, nil
		}
		refusal := func(f *tt.Fail) string {
			return fmt.Sprintf("%s %q at %d:%d", f.Code, f.Message, f.Row, f.Column)
		}
		if refusal(partFail) != refusal(wholeFail) {
			return 0, fmt.Errorf("%s: through the part %s, whole %s", name, refusal(partFail), refusal(wholeFail))
		}
		return cRefused, nil
	default:
		said := func(f *tt.Fail) string {
			if f == nil {
				return "a tree"
			}
			return fmt.Sprintf("%s %q", f.Code, f.Message)
		}
		return 0, fmt.Errorf("%s: through the part %s, whole %s", name, said(partFail), said(wholeFail))
	}
}

// TestEveryCFixtureReadsThroughThePartAsCReadsIt holds C's transducer part
// to C's reader. The part rebuilds the tree c's grammar's rules build,
// placing each operator from its own copy of C's operator table, and only
// this repository runs it, so its tree is held to the reader's here (the
// maintainer's ruling of 2026-11-13, admin ADR-33): every document of c's
// fixtures, each once, from its vendored test/spec, as Rust's
// release run reads them. Go writes a whole read's members in the order
// its reader's maps keep them, which is not the part's, so most documents
// read as the same tree in another member order here.
func TestEveryCFixtureReadsThroughThePartAsCReadsIt(t *testing.T) {
	dir := filepath.Join(vendored(t), "c", "test", "spec")
	files, err := support.LoadSpecDir(dir, nil)
	if err != nil {
		t.Fatalf("c's fixtures are vendored, at %s: %v", dir, err)
	}
	seen := map[string]bool{}
	counts := map[cOutcome]int{}
	var otherwise []string
	for _, file := range files {
		for _, row := range file.Rows {
			text := row.UnescNamed("input")
			if seen[text] {
				continue
			}
			seen[text] = true
			outcome, err := cThroughThePart(t, fmt.Sprintf("c/%s:%d", row.File, row.Line), text)
			if err != nil {
				otherwise = append(otherwise, err.Error())
				continue
			}
			counts[outcome]++
		}
	}
	if len(otherwise) > 0 {
		t.Errorf("%d documents read otherwise through the part:\n%s", len(otherwise), strings.Join(otherwise, "\n"))
	}
	got := [4]int{counts[cRead], counts[cReordered], counts[cDeep], counts[cRefused]}
	t.Logf("c's fixtures through the part: %d read as the reader reads them, %d in another member order, "+
		"%d past the default depth, %d refused alike", got[0], got[1], got[2], got[3])
	if want := [4]int{0, 137, 0, 32}; got != want {
		t.Errorf("counts %v, want %v", got, want)
	}
}
