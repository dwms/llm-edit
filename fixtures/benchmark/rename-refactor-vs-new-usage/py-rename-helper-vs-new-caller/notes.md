# py-rename-helper-vs-new-caller

A renames `compute_tax` -> `calc_tax` (module-scope). B adds `quote_total`,
a new caller that uses the old name `compute_tax`. diff3 merges silently:
the new function references a name that no longer exists.
