# py-delete-validator-vs-new-runner

A deletes `validate(payload)` (perhaps deciding validation moves
elsewhere). B adds a new `run(payload)` whose body calls `validate(payload)`
in a guard expression. diff3's two hunks (deletion at top, addition at
bottom) don't overlap; silent-clean merge produces a file where `run`
references the dead `validate`. Structured merge detects
DeleteVsNewCaller.

diff3 failure mode: **silent-clean-on-negative**.
