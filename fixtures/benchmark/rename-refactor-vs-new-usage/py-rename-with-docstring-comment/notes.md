# py-rename-with-docstring-comment

A renames `place_order -> submit_order`. B adds a `retry_order` helper
whose body mentions the old name in a comment and a call. diff3 silently
merges (the insertion is at EOF, disjoint from the rename site) but the
new helper mentions the dead name. Structured merge propagates the
rename into B's added source including the comment.

diff3 failure mode: **silent-wrong**.
