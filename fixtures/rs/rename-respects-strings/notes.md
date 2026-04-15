# rs/rename-respects-strings

Soundness fixture for `rust_ast_rename_identifier`. `user_id` appears in five positions in `lib.rs`:

1. Function parameter (`fn error_for(user_id: u64)`) — identifier → rename.
2. Comment `// ... user_id field` — `line_comment` → preserve.
3. Raw string `r#"raw user_id literal"#` — `raw_string_literal` → preserve.
4. String literal `"Log field 'user_id' failed for {}"` — `string_literal` → preserve.
5. `format!` arg `user_id` (last positional) — identifier → rename.

The textual `rename_word_boundary` would clobber positions 2-4 because `'`, `/`, ` ` are non-identifier characters. The AST walker in `rust_ast_rename_identifier` skips `line_comment`, `string_literal`, `raw_string_literal` subtrees so positions 2-4 stay intact while 1 and 5 are correctly renamed.

`agent_a` alone carries a log; this is an applier-correctness test.
