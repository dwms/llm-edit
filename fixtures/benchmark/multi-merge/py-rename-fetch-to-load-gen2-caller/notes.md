# py-rename-fetch-to-load-gen2-caller

- Gen 0: `fetch(key)` and `evict(key)` in `repo.py`.
- Gen 1 left: agent A renames `fetch → load` (module scope).
- Gen 1 right: no-op.
- Gen 2: agent C adds `refresh()` that calls the old name `fetch(key)` — C worked off gen 0 mental model.

With persistence, gen 1's rename is replayed against gen 2's `add_function` source so the new call site becomes `load(key)`. Without persistence, the caller points at a name that no longer exists. Same pattern as the reference, but richer: the new fn references *two* symbols (`fetch` and `evict`) and only one is renamed, so the rewrite must be surgical.
