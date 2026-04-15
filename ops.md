# llm-edit op format — v0 sketch

Design target: when two agents each produce a JSONL log of structured ops
against a common base, merging their logs reconstructs the correct final
tree without guessing. Ops carry *intent* (scope, kind, semantic anchors)
that text diffs throw away.

Not a finished spec. Grounded in the 8 fixtures under `fixtures/`.

## Storage layout

Each agent worktree contains:

```
.llm-edit/
  log.jsonl    # one op per line, in the order the agent performed them
```

JSONL, one JSON object per line. Lines are ordered; order matters when
two ops from the same agent touch the same anchor.

## Op envelope

Every op is a tagged union keyed by `"op"`:

```json
{"op": "<op-name>", "ts": "<ISO-8601>", "by": "agent_a", ...payload}
```

`ts` and `by` are informational. The rest is op-specific.

## Anchors — where an op happens

Line numbers are not used. Ops reference *semantic anchors* which survive
concurrent edits from the other agent:

- **AST anchor** — `{file, path: [...selectors]}` where each selector is
  either a string node-kind or an object like
  `{kind: "function_def", name: "process"}`.
- **DOM/CSS selector anchor** — `{file, selector: "section > p"}` for
  HTML; `{file, selector: ".card"}` for CSS rules.
- **Symbol-scope anchor** — `{scope: "module"|"project", symbol: "foo"}`
  for operations that target every reference to a name.

Anchors are resolved against the *current* tree when an op is applied, so
a merge can reorder ops without their positions drifting.

## v0 op vocabulary

Derived from the fixtures. Not final — additions expected as we grow the
corpus.

### `rename_symbol`
Rename a name across a scope.
```json
{"op": "rename_symbol",
 "scope": "module",          // "module" | "project"
 "file": "app.py",            // required when scope=module
 "old": "user_id", "new": "account_id",
 "kinds": ["parameter", "identifier"]}   // which AST roles to rewrite
```

### `rename_css_class`
Rename a CSS class across every referring file (CSS selectors + HTML
`class=` attributes + JSX `className=`).
```json
{"op": "rename_css_class", "scope": "project",
 "old": "btn-primary", "new": "btn-main"}
```

### `add_function`
Insert a new function at a named anchor.
```json
{"op": "add_function",
 "anchor": {"file": "app.py", "path": ["module"]},
 "after": {"kind": "function_def", "name": "delete_user"},
 "name": "ban_user",
 "source": "def ban_user(user_id, reason):\n    ..."}
```

### `add_rule`
Insert a new CSS rule.
```json
{"op": "add_rule",
 "anchor": {"file": "styles.css"},
 "selector": ".card:hover",
 "declarations": [{"prop": "border-color", "value": "blue"}]}
```

### `insert_element`
Insert an HTML element at a DOM position.
```json
{"op": "insert_element",
 "anchor": {"file": "index.html", "selector": "body"},
 "position": "last-child",
 "html": "<button class=\"btn-primary\">Download</button>"}
```

### `extract_function`
Move a range of statements inside a function body out into a new helper,
replacing the original range with a call.
```json
{"op": "extract_function",
 "source": {"file": "process.py",
            "path": ["module", {"kind": "function_def", "name": "process"}, "body"]},
 "body_range": [1, 5],     // inclusive indices of statements in body
 "new_name": "_subtotal",
 "params": ["items"],
 "returns": ["total"]}
```

### `replace_in_anchor`
Replace a substring inside a semantic anchor's body. Preferred over
line-range edits because the anchor survives another agent's structural
moves.
```json
{"op": "replace_in_anchor",
 "anchor": {"file": "process.py",
            "path": ["module", {"kind": "function_def", "name": "process"}, "body"]},
 "old": "item.price * item.quantity",
 "new": "item.price * item.quantity * 1.08"}
```

### `wrap_element`
Wrap an element's children in a new parent.
```json
{"op": "wrap_element",
 "anchor": {"file": "page.html", "selector": "section"},
 "wrapper": "<div class=\"container\"></div>"}
```

### `retype_binding`
Change a binding's keyword (`let` → `const`, `var` → `let`, etc.) with
a declared precondition.
```json
{"op": "retype_binding",
 "anchor": {"file": "counter.js",
            "path": ["module", {"kind": "function_declaration", "name": "newCounter"},
                     "body", {"kind": "lexical_declaration", "name": "count"}]},
 "from": "let", "to": "const",
 "precondition": {"reassigned": false}}
```

### `delete_node`
Delete a specific AST node identified by anchor.
```json
{"op": "delete_node",
 "anchor": {"file": "process.py",
            "path": ["module", {"kind": "function_def", "name": "process"},
                     "body", {"kind": "comment", "index": 0}]}}
```

### `sort_block`
Sort a contiguous block of sibling nodes by a key function.
```json
{"op": "sort_block",
 "anchor": {"file": "main.py", "path": ["module", "imports"]},
 "key": "alphabetic"}
```

### `add_import`
Add an import statement; compatible with `sort_block` — insertion
position is derived, not fixed.
```json
{"op": "add_import",
 "anchor": {"file": "main.py"},
 "module": "re"}
```

## Merge algorithm (sketch)

Given logs `A` and `B` over the same base tree:

1. **Classify** every op as `refactor` (`rename_symbol`,
   `rename_css_class`, `extract_function`, `wrap_element`, `sort_block`,
   `retype_binding`) or `additive` (`add_function`, `add_rule`,
   `insert_element`, `add_import`, `replace_in_anchor`).
2. **Apply A's refactors** to the base tree.
3. **Apply A's additive ops** in log order.
4. **Rewrite B's ops** through A's refactors:
   - For every `rename_symbol` / `rename_css_class` in A, rewrite
     identifiers in the bodies/attributes of B's remaining ops.
   - For every `extract_function` in A, resolve any of B's
     `replace_in_anchor` ops whose anchor's body_range fell inside the
     extracted region so they now target the new helper. Specifically:
     if B's op searches for `old` within anchor X and X no longer
     contains `old`, search the nodes A moved out of X — the anchor
     follows the content.
   - For every `wrap_element` in A, update B's DOM selectors so they
     still resolve.
   - For every `rename_symbol` in A (project or module scope), rewrite
     both the `content` *and* the `after_line_contains` needle of B's
     `insert_after` ops, so (a) B's anchor still resolves against the
     post-A file and (b) the inserted text references the new name.
     The rewrite is word-boundary-scoped and applies to
     `.js`/`.ts`/`.jsx`/`.tsx`/`.html` files.
   - For every `wrap_element` in A, check whether B's `insert_after`
     needle points at a line that lived inside the wrapped element in
     the base tree. If so, indent B's `content` by +2 spaces per
     non-blank line to match the depth shift A's wrap introduced —
     the anchor follows the content into the new wrapper.
5. **Apply B's rewritten refactors**.
6. **Apply B's rewritten additive ops**.
7. **Conflicts**: if an op's anchor fails to resolve after rewriting, or
   a precondition (e.g. `reassigned: false` on `retype_binding`) is
   invalidated by the other agent's changes, stop and emit a
   *semantic* conflict — not a text conflict — with the two ops named.

Symmetry: A and B are interchangeable. When both agents perform the same
refactor (e.g. both rename X→Y), the second application is a no-op.

## Open questions

- How does a human author write these logs? Direct JSONL is hostile;
  we'll likely offer a CLI (`llm-edit rename-symbol old new`) that
  performs the edit *and* appends to the log. Design of that CLI is the
  next big decision.
- Do we need a notion of *op dependencies* within a single agent's log
  (op 2 depends on op 1 having happened)? For v0 assume sequential
  application suffices.
- Cross-language renames (CSS class → HTML class attribute → JSX
  className) need a shared symbol table. Starting with grammar-specific
  rewriters and a hardcoded cross-language table for common cases.
