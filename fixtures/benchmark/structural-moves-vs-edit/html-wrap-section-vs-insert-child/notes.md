# html-wrap-section-vs-insert-child
A wraps section children in an <article>. B adds a new <p> after the tagline.
Diff3 conflicts on the re-indented block; structured merge routes the new
<p> into the correct wrapped position with the +2 indent A introduced.
