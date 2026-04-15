# js-project-rename-3-files-vs-new-page

A renames `trackEvent → recordEvent` project-scope across three files
(lib.js, main.js, index.html). B inserts a new anchor into a fourth
file (extra.html) using the dead name. diff3 merges each file
independently with no conflicts but extra.html ships pointing at a
function that no longer exists. Structured merge propagates the project
rename into B's insert_element html.

diff3 failure mode: **silent-wrong**.
