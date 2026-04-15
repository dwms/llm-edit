# py-delete-deprecated-gen1-gen2-add-new

- Gen 0: `new_api` and `legacy_api`.
- Gen 1 left: deletes `legacy_api`.
- Gen 1 right: no-op.
- Gen 2: adds `batch` that composes `new_api`.

Gen 2 never references `legacy_api`. Persistence doesn't change the textual payload here but the *tree state* must carry the deletion forward so that a later op looking for `legacy_api` as an anchor would correctly fail. This case is the happy path — the future-conflict variant (gen 2 tries to call the deleted fn) is in `true-conflicts/`.
