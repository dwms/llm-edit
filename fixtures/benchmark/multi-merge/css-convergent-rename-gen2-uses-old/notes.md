# css-convergent-rename-gen2-uses-old

- Gen 0: a CSS class `btn-primary` is used in HTML.
- Gen 1 left and right both rename `btn-primary → btn-main` independently (convergent). Idempotence rules say the second application is a no-op.
- Gen 2: inserts a new `<button class="btn-primary">` — the author worked off the pre-rename code.

With persistence, the rename is replayed through gen 2's inserted HTML, producing `btn-main`. Without persistence, the new button keeps the dead class and the file is visually inconsistent. Also a test that convergent renames collapse cleanly in the gen-1 merge.
