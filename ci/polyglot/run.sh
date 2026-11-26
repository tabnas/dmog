#!/usr/bin/env bash
# TypeScript and Go gate: this package's TypeScript suite, then the Go
# suite, plain and with the tabnas_nodecell build tag, and vet.
#
# By default it is standalone: npm installs the @tabnas packages from the
# registry, Go resolves go/go.mod's requirements from the module proxy
# (GOWORK=off), and the tests read the test files vendored in
# test/fixtures. Nothing beside this checkout is read.
#
# DMOG_SIBLINGS=1 runs it against sibling checkouts instead, to test
# unreleased changes (alchemy's downstream workflow does, for each change
# there): it builds each TypeScript sibling in dependency order and links
# it into the packages that take it, runs Go in a temporary workspace over
# the sibling checkouts ci/rust/run.sh names, and the tests read the
# checkouts' test files (DMOG_FIXTURES).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FLEET_ROOT=$(cd "$ROOT/.." && pwd)
# shellcheck source=../phase.sh
source "$ROOT/ci/phase.sh"

# The links a previous run left: npm installs no peer of a linked package
# (jsonic's @tabnas/json, which xml's tests import), so a run over a
# node_modules an earlier run linked would miss them. Remove them, and npm
# installs the registry copies with their peers before the links go back.
unlink_siblings() {
  [[ -d "$1/node_modules/@tabnas" ]] || return 0
  find "$1/node_modules/@tabnas" -mindepth 1 -maxdepth 1 -type l -exec rm -f {} +
}

if [[ "${DMOG_SIBLINGS:-}" != 1 ]]; then
  test_registry() {
    cd "$ROOT/ts"
    unlink_siblings "$ROOT/ts"
    npm install --ignore-scripts --no-save
    npm test
  }
  in_go() {
    cd "$ROOT/go"
    GOWORK=off "$@"
  }
  phase "typescript: test dmog (25%)" test_registry
  phase "go: test dmog (50%)" in_go go test -count=1 ./...
  phase "go: test dmog with tabnas_nodecell (75%)" in_go go test -count=1 -tags tabnas_nodecell ./...
  phase "go: vet dmog (100%)" in_go go vet ./...
  echo "polyglot gate: green"
  exit 0
fi

export DMOG_FIXTURES="$FLEET_ROOT"
echo "polyglot gate: against the sibling checkouts in $FLEET_ROOT"

SIBLINGS=$(sed -n 's/^SIBLINGS="\(.*\)"$/\1/p' "$ROOT/ci/rust/run.sh")
# The engine and the grammars, then alchemy (its build compiles src only),
# then transduce and render, which build on alchemy's shared types. Every
# grammar `translate` reads and writes is here, so TypeScript runs on the
# same sibling sources the Rust gate and the Go workspace do; ini takes
# hoover, feed takes xml, c takes expr, the grammar notations take bnf,
# and proto's build takes abnf.
TS_PACKAGES="parser support json jsonic hoover csv ini json5 jsonc jsonl markdown toml xml yaml zon chess css expr c feed bnf abnf ebnf gbnf proto semver alchemy transduce render"

for package in $TS_PACKAGES; do
  if [[ ! -f "$FLEET_ROOT/$package/ts/package.json" ]]; then
    echo "no $package TypeScript checkout at $FLEET_ROOT/$package/ts" >&2
    exit 1
  fi
done

# Replace each registry copy npm installed with a link to the sibling.
link_siblings() {
  local package_dir=$1 sibling name target
  for sibling in $TS_PACKAGES; do
    [[ "$FLEET_ROOT/$sibling/ts" != "$package_dir" ]] || continue
    name=$(cd "$FLEET_ROOT/$sibling/ts" && node -p "require('./package.json').name")
    target="$package_dir/node_modules/$name"
    [[ -e "$target" || -L "$target" ]] || continue
    rm -rf "$target"
    mkdir -p "$(dirname "$target")"
    ln -s "$FLEET_ROOT/$sibling/ts" "$target"
  done
}

install_and_build() {
  cd "$1"
  unlink_siblings "$1"
  npm install --ignore-scripts --no-save
  link_siblings "$1"
  npm run build --if-present
}

test_cli() {
  cd "$ROOT/ts"
  unlink_siblings "$ROOT/ts"
  npm install --ignore-scripts --no-save
  link_siblings "$ROOT/ts"
  npm test
}

step=0
total=$(( $(wc -w <<<"$TS_PACKAGES") + 1 ))
for package in $TS_PACKAGES; do
  step=$((step + 1))
  phase "typescript: $step of $total ($((step * 100 / total))%) install and build $package" \
    install_and_build "$FLEET_ROOT/$package/ts"
done

phase "typescript: $total of $total (100%) test dmog" test_cli

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT
(
  cd "$WORK_DIR"
  go work init
  for sibling in $SIBLINGS; do
    module="$FLEET_ROOT/$sibling/go"
    [[ -f "$module/go.mod" ]] || continue
    go work use "$module"
  done
  go work use "$ROOT/go"
)

in_go() {
  cd "$ROOT/go"
  GOWORK="$WORK_DIR/go.work" "$@"
}

phase "go: test dmog (33%)" in_go go test -count=1 ./...
phase "go: test dmog with tabnas_nodecell (67%)" in_go go test -count=1 -tags tabnas_nodecell ./...
phase "go: vet dmog (100%)" in_go go vet ./...
echo "polyglot gate: green"
