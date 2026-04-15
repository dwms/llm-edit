# js-rename-in-template-literal

A renames `fetchUser -> loadUser` (project scope). B adds a `logAndFetch`
helper that references the old name inside a template literal (backticks)
and in a call expression. The insertion region is disjoint from A's
rename site, so diff3 silently merges both hunks but leaves the new
helper pointing at the dead name — including the template-literal
reference.

diff3 failure mode: **silent-wrong**.
