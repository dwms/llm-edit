# html-wrap-list-vs-insert-list-item

A wraps the `<ul>` children in `<nav>`. B inserts a new `<li>Middle</li>`
between First and Second. Diff3 conflicts on the re-indented list items.
Structured merge shifts B's content by +2 spaces so the new `<li>` lands
inside `<nav>` at the right depth.

diff3 failure mode: **conflict-unresolved**.
