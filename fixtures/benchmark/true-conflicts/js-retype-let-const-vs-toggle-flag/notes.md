# js-retype-let-const-vs-toggle-flag

A makes `on` const. B introduces a `toggle()` method that flips `on`.
Both edits land in disjoint regions of the file, so diff3 silently
merges them. Structured merge raises RetypePreconditionViolated.

diff3 failure mode: **silent-clean-on-negative**.
