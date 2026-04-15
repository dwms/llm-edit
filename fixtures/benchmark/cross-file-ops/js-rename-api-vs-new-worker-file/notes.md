# js-rename-api-vs-new-worker-file

A renames `dispatch -> publish` project-scope across api.js and doc.html
(three files touched). B independently adds a `drainQueue()` helper in
worker.js that calls the old name. Worker.js has no conflict with A's
renames (A didn't touch it), so diff3 ships a file with a dead call.
Structured merge propagates the rename into B's insert_after content.

diff3 failure mode: **silent-wrong**.
