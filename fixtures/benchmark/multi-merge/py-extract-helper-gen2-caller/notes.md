# py-extract-helper-gen2-caller

- Gen 0: a single `total()` function that sums `item.price`.
- Gen 1 left: `extract_function` pulls the loop body into `_subtotal`; `total` now just calls `_subtotal`.
- Gen 1 right: no-op.
- Gen 2: adds `total_with_tax` that calls `total(items)`.

The gen 2 add still references the *public* `total` name, which survived the extract, so this should merge cleanly whether or not persistence replays the extract. Contrast with `extract-persists-to-gen2-edit` which tests the harder case of a gen-2 `replace_in_anchor` whose anchor content moved into the helper.
