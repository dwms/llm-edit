# py-large-imports-vs-add-middle

Larger, 7-import service file where the initial order is arbitrary. A sorts imports alphabetically. B adds a new `functools` import. After merge, the block is sorted AND includes `functools` in the correct alphabetical slot (between `collections` and `hashlib`).

Tests the `add_import_smart` + `sort_block` interaction: add-then-sort and sort-then-add must converge to the same result. Contrast with the smaller existing ordering fixtures.
