# py-add-import-convergent-gen2-uses

- Gen 0: just `import os` + a `run()` function.
- Gen 1 left: `add_import json`.
- Gen 1 right: `add_import json` (convergent with left) then `sort_block` imports alphabetically.
- Gen 2: adds `dump_state` that uses `json.dumps`.

Tests convergent no-op on `add_import`, interaction with `sort_block`, and a gen-2 new function that depends on the gen-1 import existing. No persistence needed for correctness per se, but it validates that duplicate imports don't appear and sort is applied exactly once.
