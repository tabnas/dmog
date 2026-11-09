# @tabnas/dmog

The `dmog` command for [alchemy](https://github.com/tabnas/alchemy), the
small transformation language for streaming structured data through the
tabnas parser and transducer stack.

```sh
npm install -g @tabnas/dmog
```

```
dmog canon FILE       print the program in canonical form
dmog format FILE      print the program in layout form
dmog check FILE       parse, desugar, resolve, check and build the plan
dmog explain FILE     print the plan report
dmog run [--render csv|json] [--max-output-bytes N] PROGRAM INPUT
                      run the program over the JSON document INPUT
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
to alchemy's `compile`, and `run` reads `INPUT` with `@tabnas/json`.
`translate` reads and writes the formats of the grammar packages it
carries (`@tabnas/abnf`, `c`, `chess`, `css`, `csv`, `ebnf`, `expr`,
`feed`, `gbnf`, `ini`, `json`, `json5`, `jsonc`, `jsonic`, `jsonl`,
`markdown`, `proto`, `semver`, `toml`, `xml`, `yaml` and `zon`), composing
each pair's route
from the translation parts the packages export with alchemy's `translate`;
`src/translate.ts` documents it.

This package includes its TypeScript sources under `src/` alongside the
compiled JavaScript and declarations under `dist/`.
