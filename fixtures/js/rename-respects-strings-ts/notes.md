# js/rename-respects-strings-ts

TS variant of `rename-respects-strings`. Exercises the `tree-sitter-typescript` dispatch path (separate parser from plain JS).

Same four positions, same soundness expectation: only parameter + interpolation renamed; template literal text and comment preserved.
