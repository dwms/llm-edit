# js-rename-export-vs-new-html-page

A project-renames `openPanel -> togglePanel` (widget.js + home.html). B
creates a brand-new `settings.html` page that wires two buttons to the
old name. The new page file didn't exist in base, so diff3 produces it
untouched — referencing the dead name. Structured merge propagates the
project rename into B's newly-inserted HTML attributes.

diff3 failure mode: **silent-wrong** (new file, no conflict possible).
