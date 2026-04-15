# multi-merge/extract-persists-to-gen2-edit

Gen 1 extracted the summation loop from `process` into `_subtotal`. Gen 2's agent then tried to edit a line inside what they believed was `process.body`, but the content now lives in `_subtotal.body`.

- **With persistence**: gen 1's merged log carries the `extract_function` record, but today's `collect_rewrites` reconstructs `moved_text` by inspecting `base_tree` — which in the second merge is the *post-extract* tree, so `moved_text` is empty and the anchor-follow fires no rewrite. Fix: serialize `moved_text` into the op itself when emitting the merged log, or re-derive it by reading the newly-created helper's body. Filed as future work; current bench shows this case failing even with persistence.
- **Without persistence**: `replace_in_anchor` targets `process.body`, which no longer contains `item.price` — the applier errors. Fallback to diff3 would likewise fail.
