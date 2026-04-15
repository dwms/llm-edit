# py-delete-fn-vs-new-cross-file-caller

A removes `legacy()` in `lib.py`; B adds a new `wrapper()` in `api.py` that
calls `legacy(...)`. The new caller lives in a sibling file, so the
`delete_vs_new_caller` detector must fire across files (not only when the new
caller happens to land in the same file as the deletion).
