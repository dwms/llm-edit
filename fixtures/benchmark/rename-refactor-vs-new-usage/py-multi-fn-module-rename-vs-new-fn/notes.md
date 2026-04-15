# py-multi-fn-module-rename-vs-new-fn

A 7-function Python module with repeated parameter and call-site uses of `user_id`. A renames `user_id → uid` module-wide. B independently adds `undelete_user` that also uses `user_id` as parameter and call argument.

Expected result: `user_id` appears nowhere as an identifier, but the string literal `"user_id"` (inside the `_audit` insert dict) is preserved. The new fn picks up the rename through its payload. Stresses AST-accurate renaming in a long multi-fn file where text-diff merging would otherwise be unambiguously broken.
