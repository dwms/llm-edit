# css/rename-selector-vs-new-variant

## Scenario
A single `.card` rule in a stylesheet.

## Agent A's intent
Rename `.card` → `.panel` because the component has been renamed.

## Agent B's intent
Add a `.card:hover` variant for interactive feedback.

## Why text-diff merge fails
A modifies line 1 (`.card` → `.panel`). B appends a new rule block at the end. No textual overlap → diff3 merges cleanly and produces:

```css
.panel { ... }

.card:hover { ... }   /* orphan — no .card rule in file any more */
```

The `.card:hover` rule still works in the browser if something else in the app references `.card`, but in this self-contained stylesheet it's dead code.

## Why a human also struggles
Reading the merged output, the `.card:hover` rule looks fine in isolation. You have to cross-reference that there is no base rule `.card` any more — a global property of the file, not something visible at the local hunk.

## Why structured op-logs make this trivial
- Agent A's op: `rename_css_class(.card → .panel)` — scoped to "all selectors referencing this class".
- Agent B's op: `add_rule(selector=.card:hover, declarations=...)`.
- Merger: A's rename rewrites the *selector* in B's new rule — because `.card:hover` references the `.card` class that A renamed. Output uses `.panel:hover`.
