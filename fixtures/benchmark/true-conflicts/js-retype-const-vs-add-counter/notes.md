# js-retype-const-vs-add-counter

A retypes `let n` to `const n` with precondition `reassigned: false`. B
adds a new method `incr: () => { n = n + 1; }` that reassigns `n`.
diff3's two hunks (declaration line vs new method line) don't overlap;
silent-clean merge would ship `const n = 0` next to `n = n + 1` — a
runtime error. Structured merge raises RetypePreconditionViolated.

diff3 failure mode: **silent-clean-on-negative**.
