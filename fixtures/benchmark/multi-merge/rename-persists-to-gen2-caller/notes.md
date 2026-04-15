# multi-merge/rename-persists-to-gen2-caller

## The narrative
- Gen 0: `def get(user_id): return user_id`.
- Gen 1 left: agent A renamed `user_id → account_id` across the module.
- Gen 1 right: agent B made no edits.
- After the first merge: `def get(account_id): return account_id`.
- Gen 2: agent C (working from what they thought the base was) added a caller that passes `user_id=1`.

## Why persistence matters
Gen 2's agent referenced the pre-rename parameter name. When merging gen 2 into the gen 1 merged state:

- **With persistence** — the first merge's output log carried the `rename_symbol user_id → account_id` op forward. The second merge treats that log as "A", so C's added `source` string is rewritten through the rename → `get(account_id=1)`. ✅
- **Without persistence** — the second merge sees no rename history. C's addition lands verbatim: `get(user_id=1)` against a `def get(account_id)` signature. Broken. ✗

This fixture is the minimum viable test of the metadata-across-merges idea from issue #11.
