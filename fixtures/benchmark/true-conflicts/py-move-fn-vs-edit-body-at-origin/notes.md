# py-move-fn-vs-edit-body-at-origin

A moves `normalize` from `svc.py` → `helpers.py` (delete + add). B edits the body of `normalize` at the original location (`s.strip().lower()` → `s.strip().casefold()`). The edit's intent is to improve behavior at the call site A has vacated; a smart merger would re-apply the replace against A's moved copy in `helpers.py`, but v0 has no move op to recognise the pair, so this surfaces as delete-vs-body-edit.
