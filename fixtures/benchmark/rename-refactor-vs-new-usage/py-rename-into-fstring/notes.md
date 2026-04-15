# py-rename-into-fstring

A renames `start_server -> boot_server` (module scope). B adds a `supervise()`
helper that references the old name in three places: a comment, an f-string
literal, and an actual call. Diff3 silently merges (the regions are
disjoint) but the new helper's text still says `start_server`. Structured
merge rewrites every identifier-form occurrence in B's added source.

diff3 failure mode: **silent-wrong**.
