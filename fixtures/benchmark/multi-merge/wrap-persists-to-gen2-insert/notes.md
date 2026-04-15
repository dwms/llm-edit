# multi-merge/wrap-persists-to-gen2-insert

Gen 1's left agent wrapped `<section>`'s children in `<div class="container">`. Gen 2's agent then inserts a `<p>` after `<h1>` — which now lives inside the wrapper.

- **Without WrapElement idempotence**: gen 1's merged log carries the `wrap_element` record forward. In the gen-2 merge, the persisted wrap op replays against `gen1_tree` (already wrapped) and double-wraps the section, producing nested `<div class="container">` elements. Expected fixture diverges → fail.
- **With WrapElement idempotence**: `op_is_noop_against` recognises the wrapper is already the first child of `<section>` and skips the replay. The new `insert_element` is the only effective op for gen 2, and the tree matches `expected/`.
