# py-delete-unused-fn-vs-edit-caller
A deletes deprecated_helper. B edits current's body. Text-diff may merge
cleanly but diff3 often conflicts on deletion boundary; either way structured
op merge is trivial.
