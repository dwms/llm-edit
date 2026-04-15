# js/rename-respects-strings

Single-applier soundness fixture — mirrors `fixtures/py/rename-respects-strings` for JS.

`userId` appears in four positions:
1. Parameter of `errorFor` — identifier node → rename.
2. String content `'userId'` inside a template literal — `template_chars` node → do NOT rename.
3. Comment `// ... userId field` — `comment` node → do NOT rename.
4. Template interpolation `${userId}` — identifier node inside an interpolation → rename.

Before tree-sitter-javascript, `rename_word_boundary` rewrote all four because `'`, `/`, ` ` are all non-identifier characters. The AST walker in `js_ts_ast_rename_identifier` visits only identifier-kind nodes, so positions 2 and 3 are preserved.

`agent_a` alone carries a log — this is an applier-correctness test, not a merge.
