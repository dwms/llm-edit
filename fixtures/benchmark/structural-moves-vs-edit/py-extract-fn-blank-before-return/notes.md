# py-extract-fn-blank-before-return

A extracts the loop into `_subtotal`. The base function has a blank line and a
comment between the loop body and the trailing `return total`. The applier must
scan past the blank/comment statements to absorb the trailing return into the
helper, otherwise the helper has no terminal return.

B independently edits a line inside the loop body that A moves into `_subtotal`.
A correct merge re-routes B's edit into the relocated body.
