# ts-rename-fn-vs-new-branch

A renames `parseInput → parseNumber` module-scope. B adds `handleSafe(...)` that calls the old name. Standard rename-vs-new-usage in TypeScript; stresses that the rewrite survives multi-line, multi-statement inserted content.
