# js-retype-const-gen2-add-reader

- Gen 0: `let apiUrl = "..."` inside `makeConfig`.
- Gen 1 left: retype `let → const` with `reassigned: false` precondition.
- Gen 1 right: no-op.
- Gen 2: adds another reader (`host`) that reads `apiUrl`.

Gen 2's add is another reader, so the gen-1 precondition stays valid. Persistence here is about *not* re-issuing the retype in gen 2 (it's already done) and about the precondition still holding in the multi-generation tree. A future merger that tried to re-apply the retype would fail because the `let` is already gone.
