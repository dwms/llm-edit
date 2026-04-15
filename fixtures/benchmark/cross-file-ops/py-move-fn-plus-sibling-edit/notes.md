# py-move-fn-plus-sibling-edit

A moves `serialize` from `api.py` → `codec.py` (v0 move surrogate = delete + add). B independently adds `decode` to `codec.py`. Both touch `codec.py` additively, so the merge should produce a `codec.py` with `encode`, `serialize`, and `decode` in that order.

The remaining caller in `api.py` (`handle` still calls `serialize(obj)`) becomes a dangling reference — a smart merger would inject an `import` edit, but v0 has no import-aware move tracking. Expected output preserves the dangling reference; this is a positive case the merger CAN produce today, even if the result isn't executable. Upstream fix = a real `move_function` op.
