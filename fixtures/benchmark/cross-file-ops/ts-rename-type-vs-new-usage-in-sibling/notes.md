# ts-rename-type-vs-new-usage-in-sibling
A renames interface UserRecord -> AccountRecord across project. B adds a
new list() function in repo.ts that returns UserRecord[]. Text merge
produces a file that imports AccountRecord but references UserRecord.
