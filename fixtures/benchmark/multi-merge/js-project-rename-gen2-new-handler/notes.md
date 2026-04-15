# js-project-rename-gen2-new-handler

- Gen 0: `api.js` exports `sendEvent`; `ui.js` imports and calls it.
- Gen 1 left: project-scope rename `sendEvent → track`.
- Gen 1 right: no-op.
- Gen 2: adds `onSubmit` handler after the existing click handler in `ui.js`, still calling the pre-rename `sendEvent`.

With persistence the gen-2 insert's `content` has `sendEvent` rewritten to `track`, matching the gen-1 rename. Cross-file rename + multi-file gen-2 payload stress case.
