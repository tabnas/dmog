// Copyright (c) 2026 tabnas, MIT License

package main

// The three ports build against the same releases, and the vendored test
// files are theirs. go/go.mod is the reference: each tabnas crate in
// rs/Cargo.toml must require the version go/go.mod requires of the same
// repository's Go module (npm installs the latest release, which is that
// one too), and each repository's tag in test/fixtures/SOURCES.tsv
// (tools/vendor-fixtures.sh) must be go/ and that version. A requirement
// that moves in one place and not the others fails here, naming what to
// change, rather than as tests that hold one release's code to another's
// fixtures.

import (
	"bufio"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// goModRequirements is each tabnas repository go/go.mod requires, by
// name, with its version (v-prefixed).
func goModRequirements(t *testing.T, root string) map[string]string {
	t.Helper()

	required := map[string]string{}
	mod, err := os.Open(filepath.Join(root, "go", "go.mod"))
	if err != nil {
		t.Fatal(err)
	}
	defer mod.Close()
	scan := bufio.NewScanner(mod)
	for scan.Scan() {
		fields := strings.Fields(strings.TrimPrefix(strings.TrimSpace(scan.Text()), "require "))
		if len(fields) < 2 {
			continue
		}
		repo, ok := strings.CutPrefix(fields[0], "github.com/tabnas/")
		if !ok {
			continue
		}
		if repo, ok = strings.CutSuffix(repo, "/go"); ok {
			required[repo] = fields[1]
		}
	}
	if err := scan.Err(); err != nil {
		t.Fatal(err)
	}
	return required
}

// A dependency line of rs/Cargo.toml on a tabnas crate: the key, the
// package when it is renamed, and the version.
var cargoDep = regexp.MustCompile(`^([a-z0-9-]+) = \{ (?:package = "([a-z0-9-]+)", )?version = "([^"]+)"`)

func TestCargoTomlMatchesGoMod(t *testing.T) {
	// This test lives in go/cmd/dmog, three levels below the repo root.
	root := filepath.Join("..", "..", "..")
	required := goModRequirements(t, root)
	raw, err := os.ReadFile(filepath.Join(root, "rs", "Cargo.toml"))
	if err != nil {
		t.Fatal(err)
	}
	seen := 0
	for _, line := range strings.Split(string(raw), "\n") {
		if strings.Contains(line, "path = ") && strings.Contains(line, "tabnas") {
			t.Errorf("rs/Cargo.toml takes a tabnas crate by path, not from crates.io: %s", line)
		}
		m := cargoDep.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		name := m[1]
		if m[2] != "" {
			name = m[2]
		}
		repo, ok := strings.CutPrefix(name, "tabnas-")
		if !ok {
			continue
		}
		seen++
		version, ok := required[repo]
		if !ok {
			t.Errorf("rs/Cargo.toml requires %s, but go/go.mod requires no github.com/tabnas/%s/go", name, repo)
			continue
		}
		if want := strings.TrimPrefix(version, "v"); m[3] != want {
			t.Errorf("rs/Cargo.toml requires %s %s, but go/go.mod requires %s", name, m[3], version)
		}
	}
	if seen == 0 {
		t.Fatal("rs/Cargo.toml names no tabnas crate")
	}
}

func TestVendoredFixturesMatchGoMod(t *testing.T) {
	// This test lives in go/cmd/dmog, three levels below the repo root.
	root := filepath.Join("..", "..", "..")
	required := goModRequirements(t, root)

	raw, err := os.ReadFile(filepath.Join(root, "test", "fixtures", "SOURCES.tsv"))
	if err != nil {
		t.Fatalf("no vendored fixtures: run tools/vendor-fixtures.sh: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
	if len(lines) < 2 {
		t.Fatal("test/fixtures/SOURCES.tsv names no repository")
	}
	for _, line := range lines[1:] {
		cells := strings.Split(line, "\t")
		if len(cells) != 3 {
			t.Errorf("SOURCES.tsv: not repository, ref and commit: %q", line)
			continue
		}
		repo, ref := cells[0], cells[1]
		version, ok := required[repo]
		if !ok {
			t.Errorf("%s: vendored, but go/go.mod requires no github.com/tabnas/%s/go", repo, repo)
			continue
		}
		if want := "go/" + version; ref != want {
			t.Errorf("%s: vendored at %s, but go/go.mod requires %s: run tools/vendor-fixtures.sh", repo, ref, want)
		}
	}
}
