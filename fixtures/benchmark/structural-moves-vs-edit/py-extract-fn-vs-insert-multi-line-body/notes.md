# py-extract-fn-vs-insert-multi-line-body

A extracts the loop body into a helper `_double_items`. B inserts a
debug print right after `x = it.value`. The anchor line moves into the
helper, so B's insertion must follow the content. diff3 sees overlapping
hunks: A shrinks run() to a 2-line stub and B inserts a new line in the
middle of the old body. Without structured resolution this produces
conflict markers.

diff3 failure mode: **conflict-unresolved**.
