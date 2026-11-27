# github.com/tabnas/dmog/go

The Go port of the `dmog` command (data mogrify): it translates a
document from any of the tabnas formats into any other, streaming, and
runs programs in the [alchemy](https://github.com/tabnas/alchemy)
language over JSON documents. The TypeScript and Rust ports install the
same command, and the repository's
[README](https://github.com/tabnas/dmog#readme) describes it in full.

```sh
go install github.com/tabnas/dmog/go/cmd/dmog@latest
```

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

The module has three packages:
- `cmd/dmog` is the command. Its package comment documents the failures
  and exit statuses.
- `translate` composes the route for each pair of formats from the
  translation parts their modules export, with alchemy's `translate`.
- `e2e` holds the tests of alchemy's programs: alchemy's API composed
  with transduce and render.

To develop it: every tabnas module comes from the module proxy at its
latest release, and the tests read the test files vendored under
`../test/fixtures/`. A clone of the repository is all they need. From
this directory, run `gofmt -l .`, `go vet ./...` and `go test ./...`. Add
`GOWORK=off` when a `go.work` above the checkout would take the module
in. `../ci/polyglot/run.sh` is the gate CI runs, with the TypeScript
suite. With `DMOG_SIBLINGS=1` it runs against sibling checkouts instead.
