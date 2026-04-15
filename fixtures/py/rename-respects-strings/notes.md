# py/rename-respects-strings

Single-applier soundness fixture. `user_id` appears in four distinct positions:

1. Parameter of `error_for` — **identifier** node, should rename.
2. String literal `'user_id'` inside an f-string's text — **not** an identifier, should **not** rename.
3. Comment `# ... user_id field` — **not** an identifier, should **not** rename.
4. F-string interpolation `{user_id}` — **identifier** node, should rename.

The textual `rename_word_boundary` (pre-tree-sitter) rewrote all four because `'` and `#` are non-identifier characters. The AST-scoped `py_ast_rename_identifier` only touches positions 1 and 4 — string contents and comments are separate node kinds in the tree-sitter-python grammar and are skipped by the walker.

Only `agent_a` carries a log; this is applier-correctness, not a merge test.
