# html-wrap-main-vs-insert-after-heading

A wraps `<main>` children in a `.container` div (re-indents children +2).
B inserts a new `<p class="lead">` right after the `<h1>`. diff3 produces
overlap conflicts on the re-indented lines. Structured merge detects
that B's anchor line lived inside the wrapped element, bumps B's content
indent by +2, and inserts at the new depth.

diff3 failure mode: **conflict-unresolved**.
