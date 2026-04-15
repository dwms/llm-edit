# cross/css-class-rename

## Scenario
A small web page with a CSS stylesheet and an HTML file. Two agents work concurrently.

## Agent A's intent
Rename the CSS class `.btn-primary` → `.btn-main` everywhere — both the rule in `styles.css` and every `class="btn-primary"` reference in `index.html`.

## Agent B's intent
Add a new "Download" button to `index.html`, following the existing convention by giving it `class="btn-primary"`.

## Why text-diff merge fails silently
- `styles.css`: only A edited, B untouched → clean merge, keeps A's `.btn-main`.
- `index.html`: A edited lines 4, B appended a new line 6. No textual overlap → clean merge with no conflict markers.
- Result: `index.html` contains both `class="btn-main"` (from A's renames) and `class="btn-primary"` (from B's new button). The "btn-primary" class no longer exists in CSS — broken UI, no warning.

## Why a human also struggles
This one is especially insidious cross-file. A reviewer looking at the HTML diff in isolation sees nothing wrong. Only by cross-referencing CSS changes does the problem surface.

## Why structured op-logs make this trivial
- Agent A's op: `rename_css_class(.btn-primary → .btn-main)` with declared scope = "all files referencing this class".
- Agent B's op: `insert_element(button, class=[btn-primary], text="Download")`.
- Merger: A's rename is a cross-file refactor; apply it to every class reference in B's additions too.
