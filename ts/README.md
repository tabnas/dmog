# @tabnas/dmog

The `dmog` command (data mogrify): it translates a document from any of
the tabnas formats into any other, streaming, and runs programs in the
[alchemy](https://github.com/tabnas/alchemy) language over JSON
documents. This is its TypeScript port. The Go and Rust ports install
the same command, and the repository's
[README](https://github.com/tabnas/dmog#readme) describes it in full.

```sh
npm install -g @tabnas/dmog
```

```
dmog canon FILE       print the program in canonical form
dmog format FILE      print the program in layout form
dmog check FILE       parse, desugar, resolve, check and build the plan
dmog explain FILE     print the plan report
dmog run [--grammar FORMAT] [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
                      run the program over the JSON document INPUT; with --grammar, over
                      the rule passes of INPUT parsed as FORMAT (a transducer part's input)
dmog translate --from FORMAT --to FORMAT [--path PATH] [--key KEY]
               [--with PROGRAM] [--whole] [--max-output-bytes N] INPUT
                      write INPUT, read as FORMAT, in another format
dmog formats          print the formats translate reads and writes, as JSON
```

A `FILE`, `PROGRAM` or `INPUT` may be `-` for standard input. A failure is
one JSON object on standard error, and the exit status follows its code;
`src/cli.ts` documents both.

The command composes `@tabnas/alchemy` with the stages it compiles onto:
`@tabnas/transduce`'s `routers` and `@tabnas/render`'s `renderers`, passed
to alchemy's `compile`. `run` reads `INPUT` with `@tabnas/json`, through
JSON's transducer part. With `--grammar`, it reads the rule passes of
`INPUT` parsed with that format's grammar instead. `translate` reads and
writes the formats of the grammar packages it carries (`@tabnas/abnf`,
`c`, `chess`, `css`, `csv`, `ebnf`, `expr`, `feed`, `gbnf`, `ini`, `json`,
`json5`, `jsonc`, `jsonic`, `jsonl`, `markdown`, `proto`, `semver`, `toml`,
`xml`, `yaml` and `zon`). It composes each pair's route from the
translation parts those packages export, with alchemy's `translate`, and
`src/translate.ts` documents how.

This package includes its TypeScript sources under `src/` alongside the
compiled JavaScript and declarations under `dist/`.
