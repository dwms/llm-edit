# css-long-lived-design-system-gen2-new-component

Long-lived design-system branch: gen 1 left renames two classes (`card → panel`, `btn → button`) and adds a hover rule; gen 2 introduces a compound component that references *both* old class names, in HTML AND in a CSS descendant selector.

With persistence the rewriter must:
- Replace `card` and `btn` inside the gen-2 `insert_element` HTML string.
- Rewrite the `.card .btn` selector in gen-2's `add_rule` to `.panel .button` (both tokens, same selector).

This is where class-rename persistence pays rent: without it, the new component ships with dead classes and the new rule never matches anything.
