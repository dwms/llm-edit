# py-rename-in-dict-values-and-comments

A renames `ping_host -> probe_host` (module scope). B adds a
`build_dispatch()` function whose body references the old name in a
comment and as three dict values (including a nested dict). Diff3 merges
cleanly — hunks don't collide — but the new function keeps the dead name.

diff3 failure mode: **silent-wrong**.
