# py-rename-deeply-nested-call

A renames module-scope `normalize -> canonicalize`. B adds `run_grouped()`
whose body calls the old name three times inside a nested dict/list/gen
comprehension. Rewrite regions are disjoint, so diff3 silently produces
a merged file whose new helper references the dead name in three places.

diff3 failure mode: **silent-wrong**.
