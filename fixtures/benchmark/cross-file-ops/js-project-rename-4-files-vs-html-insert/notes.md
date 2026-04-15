# js-project-rename-4-files-vs-html-insert

A renames `checkAuth → isLoggedIn` project-scope across four files
(auth.js, login.js, header.html, footer.html). B inserts a new
`<button onclick="checkAuth(currentUser)">` into a fifth file
(dashboard.html). Each file diffs cleanly under diff3, but dashboard
ships pointing at the dead name. Structured merge propagates the
project rename into B's insert_element html.

diff3 failure mode: **silent-wrong**.
