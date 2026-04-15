# css-rename-class-3-pages-vs-new-section

A renames CSS class `.row → .flex-row` project-wide across styles.css,
home.html, about.html (3 files touched). B inserts a new `<section
class="row">` into a fourth file (contact.html). Each file's diff3
merge is clean, but the new section keeps the dead class. Structured
merge propagates the project class-rename into B's insert html.

diff3 failure mode: **silent-wrong**.
