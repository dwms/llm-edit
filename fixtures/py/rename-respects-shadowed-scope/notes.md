# py/rename-respects-shadowed-scope

Soundness fixture for `rename_symbol` (Python) under nested-function
shadowing. `user_id` appears in two scopes:

1. **Outer function `outer`** — parameter, two uses (operand of `+`,
   argument to `inner(...)`). All three should be renamed.
2. **Inner function `inner`** — parameter and body use. NEITHER should
   be renamed: in Python, `inner`'s parameter binds `user_id`
   independently of `outer`'s; references inside `inner` resolve to
   that parameter via the LEGB rule, not to the outer binding.

Before #6, `py_ast_rename_identifier` walked every identifier node and
rewrote every match — including inner's parameter and body. The
scope-aware version now skips the entire subtree of any
`function_definition` whose parameter list shadows the renamed name.

Limitations tracked on #6:
- For-loop / with-as shadowing isn't recognized.
- Local assignments in non-function scopes don't shadow.
- Class scopes aren't honored separately.
- `nonlocal old` and `global old` declarations aren't read.

Single-applier soundness test (no merge).
