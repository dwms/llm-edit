# html-wrap-form-vs-insert-multi-input

A wraps the `<form>` children in a `<fieldset>`. B inserts two new
`<input>` rows between the existing email input and the submit button.
Diff3 produces overlap conflicts on the re-indented lines. Structured
merge applies the +2 indent shift to both lines of B's multi-line
content so they sit inside the new `<fieldset>` at correct depth.

diff3 failure mode: **conflict-unresolved**.
