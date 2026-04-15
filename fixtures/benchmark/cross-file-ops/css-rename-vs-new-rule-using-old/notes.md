# css-rename-vs-new-rule-using-old
A renames .widget -> .component across project. B adds a .widget.featured
variant rule in the same CSS file. diff3 merges cleanly leaving orphan
selector; structured merge rewrites B's selector too.
