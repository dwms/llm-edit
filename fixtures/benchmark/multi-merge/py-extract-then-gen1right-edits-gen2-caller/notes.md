# py-extract-then-gen1right-edits-gen2-caller

- Gen 0: `emit(payload)` in svc.py.
- Gen 1 left: A renames `emit → publish` AND adds `shutdown()` after emit.
- Gen 1 right: B independently adds `healthcheck()` after emit, body
  calls `emit({"ping": 1})`. Compound across both gen-1 agents.
- Gen 2: C adds `retry_emit()` after shutdown that retries `emit(payload)`.

Persistence threads the gen-1 rename through gen-1 right's add (ensuring
`healthcheck` calls `publish`) and through gen-2's add (rewriting
`retry_emit`'s body). Gen-2's `after.name=shutdown` resolves because
`shutdown` is in the gen-1 merged tree.

diff3 failure mode: **n/a (multi-merge)**.
