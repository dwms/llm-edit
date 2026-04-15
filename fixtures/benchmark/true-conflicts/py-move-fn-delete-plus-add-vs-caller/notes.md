# py-move-fn-delete-plus-add-vs-caller

A "moves" `helper` from `main.py` to `utils.py` using `delete_function` + `add_function` (v0 has no dedicated `move_function` op). B independently adds `run_twice` in `main.py` that calls the old local `helper`.

This is a genuine conflict with the v0 op vocabulary: the merger sees A delete `helper` in `main.py` and B add a caller that references the deleted symbol (equivalent to the existing delete-fn-vs-new-caller case). A future merger that understands deletes-plus-adds as a move could automatically rewrite B's call site to import from `utils`, but that requires a new op or cross-file coordination.
