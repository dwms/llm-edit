# py-large-fn-rename-vs-new-caller

A 27-line `compute_total` function with nested loops and conditionals, plus two auxiliary functions. A renames `compute_total → calculate_total` (module-scope Python AST rename). B adds a third consumer `total_for_report` that calls the old name.

Validates AST-based rename survives long multi-statement bodies and doesn't touch shadow variables (`subtotal`, `row_total`, `total`, etc.) — only function-identifier uses. Larger than the baseline rename fixtures so stresses AST traversal.
