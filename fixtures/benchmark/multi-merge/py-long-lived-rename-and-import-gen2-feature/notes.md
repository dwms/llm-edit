# py-long-lived-rename-and-import-gen2-feature

Long-lived branch: gen 1 left does a *series* of refactors (two module-scope renames, one add_import, one sort_block); gen 1 right is idle; gen 2 adds a new feature function built on the pre-refactor vocabulary.

Everything the gen-2 feature touches — its `after` anchor `flush`, its body's `record(...)` and `flush()` calls, even the position of its insertion — is a casualty of gen 1's refactors. With full persistence we expect the anchor to bind to the renamed `drain`, and the body calls to resolve to `emit` and `drain`. A partial-persistence merger may rewrite bodies but fail to resolve the renamed anchor — that's the interesting failure mode this fixture probes.
