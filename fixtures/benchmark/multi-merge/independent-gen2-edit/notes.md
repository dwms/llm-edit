# multi-merge/independent-gen2-edit

Gen 1 renames `user_id → account_id` in `foo`. Gen 2 adds a new function `baz` that references only `order_id` (unrelated symbol). Persistence should be a no-op here — both configs should produce the same correct output. This fixture exists to verify that persistence doesn't introduce regressions on cases where it's irrelevant.
