# js-rename-helper-vs-new-test-file

A renames `formatDate -> toIsoString` project-scope (helpers.js). B adds
two test cases in `tests.js`, each referencing the old name in its
description string and call. diff3 merges each file cleanly and ships
a test file that calls a function that no longer exists. Structured
merge rewrites B's insert_after content through the project rename.

diff3 failure mode: **silent-wrong**.
