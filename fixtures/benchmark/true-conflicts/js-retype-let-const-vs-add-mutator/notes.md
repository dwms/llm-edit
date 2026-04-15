# js-retype-let-const-vs-add-mutator

A makes `inner` const because today it's never reassigned. B adds a
`swap(v)` method that reassigns it. diff3 sees two disjoint hunks (the
declaration line and the new method line) and merges them silently into
a runtime-broken combination. Structured merge raises
RetypePreconditionViolated.

diff3 failure mode: **silent-clean-on-negative**.
