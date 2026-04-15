# js-retype-let-const-vs-new-fn

A retypes `let value` to `const value` (precondition: never reassigned).
B adds a `write(v)` method that assigns to `value`. Hunks don't overlap
in the file, so diff3 silently merges into broken code. Structured
merge raises RetypePreconditionViolated.

diff3 failure mode: **silent-clean-on-negative**.
