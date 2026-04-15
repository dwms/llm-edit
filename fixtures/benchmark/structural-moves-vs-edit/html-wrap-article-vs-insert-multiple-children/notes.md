# html-wrap-article-vs-insert-multiple-children

A wraps `<article>` children in `<section class="body">`. B inserts TWO
new child lines (`<time>` and `<small>`) right after `<h2>Story One</h2>`.
Diff3 produces overlapping hunks on the re-indented children and emits
conflict markers. Structured merge applies the +2 indent shift to both
lines of B's content and inserts them at the new depth.

diff3 failure mode: **conflict-unresolved**.
