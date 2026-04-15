# py-rename-and-extract-gen2-uses-old

- Gen 0: `total(items)` summing a list.
- Gen 1 left: A extracts the loop into `_sum_loop` THEN renames
  `total → sum_all` (compound: extract + rename).
- Gen 1 right: empty.
- Gen 2: C adds `avg(items)` that calls the dead pre-rename `total(items)`
  and uses pre-rename `after.name=total`.

Persistence rewrites C's `after.name` to `sum_all` and rewrites C's
source body so the call becomes `sum_all(items)`.

diff3 failure mode: **n/a (multi-merge)**.
