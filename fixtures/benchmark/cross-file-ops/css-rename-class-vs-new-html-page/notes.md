# css-rename-class-vs-new-html-page

A project-renames CSS class `.hero -> .banner` (styles.css + landing.html).
B inserts a `<div class="hero">` into `about.html` — a disjoint file.
diff3 sees no conflict in about.html (A didn't touch it) and ships the
div with the dead class. Structured merge rewrites B's insert_element
html through the project class-rename table.

diff3 failure mode: **silent-wrong**.
