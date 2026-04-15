# cross/js-fn-rename-vs-new-handler

## Scenario
A JS file exposes `handleClick` globally; an HTML file wires a button's `onclick` to it.

## Agent A's intent
Rename `handleClick` → `onSubmit` both in `app.js` and in every inline `onclick` attribute that references it.

## Agent B's intent
Add a second `<button>` that also fires `handleClick()`, following the existing pattern.

## Why text-diff merge fails
- `app.js`: only A touched → clean, contains `onSubmit`.
- `index.html`: A edited the existing `<button>`, B inserted a new `<button>` on the following line. Potentially clean merge or narrow conflict, depending on hunk boundaries.

If it merges cleanly (typical), the HTML ends up with one button calling `onSubmit()` and one calling `handleClick()` — the second is now a broken reference (no such function exported by app.js).

## Why a human also struggles
This is the cross-file trap: the HTML diff looks innocent in isolation ("added a button, copied the pattern"). Only by reading the JS diff does the broken reference surface. Easy to miss.

## Why structured op-logs make this trivial
- Agent A's op: `rename_symbol(handleClick → onSubmit, scope=project, include=[js-functions, html-event-handlers])`.
- Agent B's op: `insert_element(button, onclick="handleClick()", text="Again", after=first_button)`.
- Merger: A's rename is a project-scoped refactor; apply it to the *values* of `onclick` attributes in B's new element. Output: `onclick="onSubmit()"`.
