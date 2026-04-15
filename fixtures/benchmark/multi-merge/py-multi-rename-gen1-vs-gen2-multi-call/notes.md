# py-multi-rename-gen1-vs-gen2-multi-call

- Gen 0: three CRUD-style functions `get_item`, `put_item`, `has_item`.
- Gen 1 left: A renames both `get_item → read_item` and
  `put_item → write_item`.
- Gen 1 right: B adds a new `size()` function.
- Gen 2: C adds `copy_to()` which calls both pre-rename names.

Persistence: gen-1's two renames are recorded, gen-2's add_function
source is rewritten so both calls land on the new names. The gen-1
`size()` add must also be visible at gen-2 time so C's `after.name=size`
anchor resolves.

diff3 failure mode: **n/a (multi-merge)**.
