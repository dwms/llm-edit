# py-large-fn-replace-in-body-vs-add-sibling

A 25-line `build_report` with nested loops and a second helper `pretty_print`. A applies a `replace_in_anchor` inside `build_report.body` that adds a tax multiplier. B adds a sibling `export_csv` after `pretty_print`.

Expected result has both changes. The anchor-scoped replace must bind to the right function body despite its length; B's insert must land at the correct bottom position. Non-conflicting by design — both edits should compose cleanly.
