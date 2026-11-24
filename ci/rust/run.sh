#!/usr/bin/env bash
# Rust gate. Kept in one script so local and hosted validation cannot
# quietly drift apart: .github/workflows/rust.yml runs this file, and so can
# you. This is the full gate.
#
# By default it is standalone: every tabnas crate resolves from crates.io
# at the version rs/Cargo.toml requires, exactly as rs/Cargo.lock records
# it (`--locked`), and the tests read the test files vendored in
# test/fixtures. Nothing beside this checkout is read.
#
# DMOG_SIBLINGS=1 runs it against sibling checkouts instead, to test
# unreleased changes (alchemy's downstream workflow does, for each change
# there): every tabnas crate in SIBLINGS is patched to its checkout next to
# this one (a [patch.crates-io] passed to cargo, so the committed manifest
# never names a path), and the tests read the checkouts' test files
# (DMOG_FIXTURES). rs/Cargo.lock is restored afterwards.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
# shellcheck source=../phase.sh
source "$ROOT/ci/phase.sh"

# Every tabnas crate in the graph, which DMOG_SIBLINGS=1 patches to its
# checkout: alchemy, transduce and render, the JSON grammar, the engine,
# and csv and jsonic, which transduce's line sources take; then the other
# grammars `translate` reads and writes (ini takes hoover, and the grammar
# notations take bnf, their shared compiler); and this crate's own
# dev-dependency, the fixture runner. ci/polyglot/run.sh and alchemy's
# downstream workflow read this list.
SIBLINGS="parser json jsonic csv alchemy transduce render support hoover ini json5 jsonc jsonl markdown toml xml yaml zon chess css expr feed proto semver bnf abnf ebnf gbnf c"

# TypeScript- and Go-only additions needed by ci/polyglot/run.sh: none.
TS_SIBLINGS=""

cd "$ROOT/rs"

# Run through the MSRV toolchain when one is available; loud when it is
# not, because a newer toolchain accepts what the MSRV rejects.
MSRV=$(awk -F'"' '/^rust-version = /{print $2; exit}' Cargo.toml)
CARGO=(cargo)
if [[ -n "$MSRV" ]] && command -v rustup >/dev/null 2>&1; then
  # The INSTALLED toolchain's full name (`1.85.1-x86_64-...`), not the
  # `1.85` channel: `cargo +1.85` names a release channel, which rustup
  # would try to synchronize over the network even when 1.85.1 is already
  # installed, and an offline checkout would fail before cargo ran.
  TOOLCHAIN=$(rustup toolchain list 2>/dev/null | awk -v m="$MSRV" 'index($1, m) == 1 { print $1; exit }')
  if [[ -n "$TOOLCHAIN" ]]; then
    CARGO=(cargo "+$TOOLCHAIN")
  else
    echo "warning: MSRV $MSRV is not installed; running on $(rustc --version 2>/dev/null)" >&2
    echo "         install it with: rustup toolchain install $MSRV" >&2
  fi
elif [[ -n "$MSRV" ]]; then
  echo "warning: rustup is not installed; running on $(rustc --version 2>/dev/null)" >&2
  echo "         install rustup and toolchain $MSRV to validate the MSRV locally" >&2
fi

# The lock's entry for THIS crate must match the manifest, before any cargo
# command gets a chance to fix it up.
CRATE=$(awk -F'"' '/^name = /{print $2; exit}' Cargo.toml)
WANT=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
HAVE=$(awk -v c="$CRATE" -F'"' '
  $0 == "name = \"" c "\"" { f = 1; next }
  f && /^version = / { print $2; exit }
' Cargo.lock)
if [[ "$WANT" != "$HAVE" ]]; then
  echo "Cargo.lock records $CRATE ${HAVE:-<missing>}, but Cargo.toml says $WANT" >&2
  echo "run a cargo command and commit the updated rs/Cargo.lock" >&2
  exit 1
fi

# fmt reads only this crate, so it never takes the sibling patch.
FMT=("${CARGO[@]}")
# `${LOCKED[@]+...}` below: bash 3.2, macOS's, calls an empty array unbound.
if [[ "${DMOG_SIBLINGS:-}" == 1 ]]; then
  FLEET_ROOT=$(cd "$ROOT/.." && pwd)
  PATCH=$(mktemp)
  LOCK_BEFORE=$(mktemp)
  cp Cargo.lock "$LOCK_BEFORE"
  trap 'cp "$LOCK_BEFORE" Cargo.lock; rm -f "$PATCH" "$LOCK_BEFORE"' EXIT
  echo "[patch.crates-io]" > "$PATCH"
  for SIBLING in $SIBLINGS; do
    manifest="$FLEET_ROOT/$SIBLING/rs/Cargo.toml"
    if [[ ! -f "$manifest" ]]; then
      echo "DMOG_SIBLINGS=1: no $SIBLING checkout at $FLEET_ROOT/$SIBLING/rs" >&2
      echo "clone https://github.com/tabnas/$SIBLING as a sibling of $(basename "$ROOT")" >&2
      exit 1
    fi
    name=$(awk -F'"' '/^name = /{print $2; exit}' "$manifest")
    echo "$name = { path = \"$FLEET_ROOT/$SIBLING/rs\" }" >> "$PATCH"
  done
  CARGO+=(--config "$PATCH")
  LOCKED=()
  export DMOG_FIXTURES="$FLEET_ROOT"
  echo "gate: against the sibling checkouts in $FLEET_ROOT"
else
  LOCKED=(--locked)
fi

# Not `--all` on fmt: it would reach into a patched sibling checkout.
phase "gate: fmt" "${FMT[@]}" fmt --check
phase "gate: build" "${CARGO[@]}" build ${LOCKED[@]+"${LOCKED[@]}"} --all-targets
phase "gate: test" "${CARGO[@]}" test ${LOCKED[@]+"${LOCKED[@]}"} --all-targets
# The cross product of every format's own fixture corpus and the grammar
# notations' example grammars (tests/translate_test.rs, ignored in a debug
# run): about 84,000 pairs, minutes in release.
phase "gate: the formats' fixtures, every format into every other (release)" \
  "${CARGO[@]}" test ${LOCKED[@]+"${LOCKED[@]}"} --release --test translate_test -- --ignored --nocapture
# No `test --doc`: the crate is a binary with no library target, which
# cargo refuses to doc-test.
phase "gate: clippy" "${CARGO[@]}" clippy ${LOCKED[@]+"${LOCKED[@]}"} --all-targets --all-features -- -D warnings
echo "gate: green"
