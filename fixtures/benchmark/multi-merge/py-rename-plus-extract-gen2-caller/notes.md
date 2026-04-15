# py-rename-plus-extract-gen2-caller

- Gen 0: `fetch_data(key)` and `parse(raw)` in `svc.py`.
- Gen 1 left: A renames `fetch_data → load_data` (module scope).
- Gen 1 right: empty.
- Gen 2: C adds `warm_cache()` which iterates `[fetch_data(k) for k in keys]`,
  using the dead pre-rename name.

With persistence the gen-1 rename is replayed against gen-2's add_function
source and `warm_cache` ends up referencing `load_data`. diff3 has no
chained-merge analog; bench scores it as zero.

diff3 failure mode: **n/a (multi-merge bucket; diff3 always 0)**.
