# js-project-rename-twice-gen2-html

- Gen 0: two functions `getUser`, `getOrder` and one HTML page wiring
  buttons to each.
- Gen 1 left: A renames both project-wide (`getUser → loadUser`,
  `getOrder → loadOrder`).
- Gen 1 right: empty.
- Gen 2: C inserts two new HTML buttons that reference the dead names.

Persistence chains both renames forward through gen 2, rewriting both
new button onclick attributes. Diff3 has no chained-merge story.

diff3 failure mode: **n/a (multi-merge)**.
