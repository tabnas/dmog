# GBNF conformance corpus

The `.gbnf` files here are llama.cpp's own grammars, copied verbatim from
[`ggml-org/llama.cpp`](https://github.com/ggml-org/llama.cpp/tree/master/grammars)
at commit `296538adea19ff397100bf8e7d71f302d2d2bb35` (release b11589, fetched
2026-10-13; all eight files verified byte-identical to the b11497 fetch
at `ff30363a0e3e2630828309ef5a37da5d1d40396c`).
They are the reference for what real GBNF looks like, so they are
tracked as-is: no reformatting, no trimming, no "fixing" a grammar that
this compiler finds hard.

| File | What it constrains | Constructs it exercises |
|---|---|---|
| `arithmetic.gbnf` | infix arithmetic | grouping, `*`, `+`, a class starting with `-` (`[-+*/]`) |
| `c.gbnf` | a small C subset | nine-way alternation, nested groups, `[^\n]`, `[^*]` |
| `chess.gbnf` | PGN algebraic notation | `?` on groups and classes, `#` inside a class (`[+#]`) |
| `english.gbnf` | ASCII text | a class holding most of ASCII punctuation, incl. `[`, `\`, `]` escapes |
| `japanese.gbnf` | kana and CJK text | non-ASCII class ranges, dashed rule names |
| `json.gbnf` | strict JSON | empty first alternative, `{m,n}`, negated classes, `\xXX` escapes |
| `json_arr.gbnf` | a JSON array | the same, plus multi-character literals containing `\n` |
| `list.gbnf` | a bulleted list | `\uXXXX` escapes in a negated class |

`ts/test/corpus.test.js` asserts that every file compiles to a
`GrammarSpec`, accepts a spread of valid samples, and rejects
near-miss invalid ones. The one construct that still fails (chess's
`Nf3`, which needs backtracking over stacked optionals) is pinned as an
expected failure and recorded in
[`ts/doc/known-gaps.md`](../../ts/doc/known-gaps.md) — the test is
never narrowed to make a gap disappear.

Copyright for these grammar files stays with the llama.cpp project (MIT).
