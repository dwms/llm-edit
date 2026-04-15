# py-rename-and-add-import-gen2-uses-both

- Gen 0: `proc.py` with `import os` and two functions `collect/aggregate`.
- Gen 1 left: A renames `collect → snapshot` AND adds `import json`
  (compound: rename + import).
- Gen 1 right: empty.
- Gen 2: C adds `export()` that calls the dead `collect()` and uses
  `json.dumps`.

Persistence chains both gen-1 ops forward: `export`'s body gets
`collect` → `snapshot` rewrite, and the `import json` line is already
present so C's reference to `json` resolves.

diff3 failure mode: **n/a (multi-merge)**.
