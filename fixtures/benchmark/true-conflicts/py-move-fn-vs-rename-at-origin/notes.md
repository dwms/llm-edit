# py-move-fn-vs-rename-at-origin

A moves `square` from `math.py` to `util.py` (delete + add, the v0 surrogate for a move op). B renames `square → sq` in `math.py`. The two refactors disagree about the symbol's fate at the origin: A deletes it, B renames it. Must conflict.

Same shape as the existing `py-rename-vs-delete-same-target` case but with a cross-file add that a smarter merger could treat as a move.
