# css-rename-3-css-files-vs-new-html

A renames `.tag → .label` project-wide across three CSS files
(main.css, print.css, dark.css). B inserts a `<span class="tag">`
into a fourth file (page.html). diff3 sees no conflict in any file
and ships the new span with the dead class. Structured merge applies
the class-rename to B's insert_element html.

diff3 failure mode: **silent-wrong**.
