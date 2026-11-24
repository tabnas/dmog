#!/usr/bin/env bash
# Vendor the test files dmog's tests read from other tabnas repositories
# into test/fixtures/<repository>/, at the same paths they have there, so
# that every port (ts/, go/, rs/) reads them from this checkout alone.
#
#   tools/vendor-fixtures.sh [REF]
#
# Each repository is cloned from GitHub, sparsely, at the release this
# repository builds against: the tag go/vX.Y.Z of the version go/go.mod
# requires, which is the version npm installs too. REF, when given, is
# taken for every repository instead (main, say, to try unreleased
# fixtures; go/cmd/dmog's fixtures test then fails until they are released
# and re-vendored). Only the paths below are copied, and
# test/fixtures/SOURCES.tsv records the ref and commit each came from.
# Re-run it whenever go/go.mod moves a tabnas requirement.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST="$ROOT/test/fixtures"
OVERRIDE="${1:-}"
ORIGIN="${TABNAS_ORIGIN:-https://github.com/tabnas}"

# repository: the paths read, relative to its root.
SOURCES=(
  "alchemy: test/spec"
  "transduce: rs/tests/fixtures rs/tests/support"
  "abnf: test/spec ts/test/grammar"
  "ebnf: test/spec ts/test/grammar"
  "gbnf: test/spec test/corpus"
  "c: test/spec ts/test/csmith-corpus"
  "jsonc: test/spec test/JSONTestSuite"
  "yaml: test/spec test/yaml-test-suite"
  "chess: test/spec"
  "css: test/spec"
  "csv: test/spec"
  "expr: test/spec"
  "feed: test/spec"
  "ini: test/spec"
  "json: test/spec"
  "json5: test/spec"
  "jsonic: test/spec"
  "jsonl: test/spec"
  "markdown: test/spec"
  "proto: test/spec"
  "semver: test/spec"
  "toml: test/spec"
  "xml: test/spec"
  "zon: test/spec"
)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

rm -rf "$DEST"
mkdir -p "$DEST"
printf 'repository\tref\tcommit\n' > "$DEST/SOURCES.tsv"

total=${#SOURCES[@]}
n=0
for entry in "${SOURCES[@]}"; do
  n=$((n + 1))
  repo=${entry%%:*}
  read -r -a paths <<< "${entry#*:}"
  REF=$OVERRIDE
  if [[ -z "$REF" ]]; then
    version=$(awk -v m="github.com/tabnas/$repo/go" '$1 == m { print $2 }' "$ROOT/go/go.mod")
    if [[ -z "$version" ]]; then
      echo "vendor: go/go.mod requires no github.com/tabnas/$repo/go" >&2
      exit 1
    fi
    REF="go/$version"
  fi
  echo "vendor: $n of $total ($((n * 100 / total))%) $repo @ $REF"

  clone="$WORK/$repo"
  git -c advice.detachedHead=false clone -q --depth 1 --branch "$REF" --filter=blob:none --sparse \
    "$ORIGIN/$repo.git" "$clone"
  git -C "$clone" sparse-checkout set --no-cone "${paths[@]/#//}"

  for path in "${paths[@]}"; do
    if [[ ! -e "$clone/$path" ]]; then
      echo "vendor: $repo has no $path at $REF" >&2
      exit 1
    fi
    mkdir -p "$DEST/$repo/$(dirname "$path")"
    cp -R "$clone/$path" "$DEST/$repo/$path"
  done
  printf '%s\t%s\t%s\n' "$repo" "$REF" "$(git -C "$clone" rev-parse HEAD)" >> "$DEST/SOURCES.tsv"
done

# Only what the tests read of the two vendored third-party suites: the
# documents meant to parse of JSONTestSuite (y_*), and each YAML Test
# Suite case's input and error marker. Their licences stay with them.
find "$DEST/jsonc/test/JSONTestSuite/test_parsing" -type f ! -name 'y_*' -delete
# The suite's name/ and tags/ indexes are symbolic links to the same cases,
# which the tests skip and a Windows checkout cannot hold.
find "$DEST" -type l -delete
find "$DEST/yaml/test/yaml-test-suite" -type f \
  ! -name in.yaml ! -name error ! -iname 'LICENSE*' ! -iname 'README*' -delete
find "$DEST/yaml/test/yaml-test-suite" -type d -empty -delete

echo "vendor: done, $(find "$DEST" -type f | wc -l | tr -d ' ') files, $(du -sh "$DEST" | cut -f1)"
