# tabnas-dmog (Rust)

The `tabnas-dmog` crate, binary `dmog` (data mogrify): it translates a
document from any of the tabnas formats into any other, streaming, and
runs programs in the [alchemy](https://github.com/tabnas/alchemy)
language over JSON documents. It compiles every program with
[tabnas-transduce](https://github.com/tabnas/transduce)'s routers and
[tabnas-render](https://github.com/tabnas/render)'s renderers, which
alchemy's runtime takes by injection. `run` reads its document through
JSON's transducer part, a window at a time. `translate` reads a document
through its format's part, where the format ships one, and writes as it
reads. The
TypeScript and Go ports install the same command. The repository's
[README](https://github.com/tabnas/dmog#readme) describes the commands
in full, and its [AGENTS.md](https://github.com/tabnas/dmog/blob/main/AGENTS.md)
how they are tested.

```text
dmog canon FILE       print the program in canonical form
dmog format FILE      print the program in layout form
dmog check FILE       parse, desugar, resolve, check and build the plan; print nothing and exit 0
dmog explain FILE     print the plan report
dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
                      run the program over the JSON document INPUT; with --grammar, over
                      the rule passes of INPUT parsed as FORMAT (a transducer part's input)
dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
               [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT
                      write INPUT, read as FORMAT, in another format
dmog formats          print the formats translate reads and writes, as JSON
```

Install it from crates.io with `cargo install tabnas-dmog` (Rust 1.85 or
later).

To develop it: every tabnas crate comes from crates.io at its latest
release, which `Cargo.lock` pins, and the tests read the test files
vendored under `../test/fixtures/`. A clone of the repository is all
they need. From this directory, run `cargo build`, then
`cargo test --all-targets` (`tests/cli_test.rs` runs the built binary as
a script runs it), then
`cargo clippy --all-targets --all-features -- -D warnings`.
`../ci/rust/run.sh` is the gate CI runs. It adds `cargo fmt --check` and
the release run of the translation matrix, and runs every step
`--locked`. With `DMOG_SIBLINGS=1` it runs against sibling checkouts of
alchemy, transduce, render and the grammars instead.
