# html-wrap-div-vs-insert-in-middle

A wraps the `<aside>` children in a `<header>`. B inserts a `<span>Middle</span>`
between the existing Top and Bottom spans. Diff3 overlaps on the
re-indented children and emits conflict markers. Structured merge shifts
B's content by +2 to match the new `<header>` depth.

diff3 failure mode: **conflict-unresolved**.
