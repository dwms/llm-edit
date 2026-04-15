# css-rename-utility-vs-new-rule
A renames .action -> .btn project-wide. B adds a .action:disabled variant.
diff3 merges silently leaving an orphan .action:disabled selector.
