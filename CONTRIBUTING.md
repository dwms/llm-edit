# Contributing — using llm-edit on llm-edit

This repo dogfoods its own merge tool. The convention is small.

## Merging PRs

Use `llm-edit git-merge <branch>` instead of `gh pr merge` for any PR
that should land on `master`. From a clean checkout of `master`:

```sh
git fetch && git checkout master && git pull
llm-edit git-merge feat/issue-NN-foo
git push
```

This:

1. Reads both sides' source AND `.llm-edit/log.jsonl` from git's object
   store.
2. For each file, picks the semantic-merge result (when recorded ops
   describe both sides' changes) or falls back to `diff3 -m` text
   merge (when at least one side has unrecorded edits). Cross-file
   propagation from logged ops still applies in either case.
3. Writes the merged source + the merged op-log back to the working
   tree.
4. Creates a two-parent merge commit.

Exit codes mirror `git merge`: `0` clean, `1` conflicts (text or
semantic). On semantic conflict the report lands at
`.llm-edit/conflicts.txt`; on text conflict the affected files have
`<<<<<<<` markers.

## Editing

Use the recording CLI when your change matches an existing op:

| Change | Command |
| --- | --- |
| Rename a Python/JS/TS symbol | `llm-edit rename-symbol --file F OLD NEW` |
| Rename a CSS class project-wide | `llm-edit rename-css-class OLD NEW` |
| Add `import X` | `llm-edit add-import --file F X` |
| Sort the import block | `llm-edit sort-block --file F` |

For everything else, edit normally with whatever tool you use. The
hybrid merger handles unrecorded edits correctly via text fallback.
The op vocabulary will grow over time
([#issue-list](https://github.com/dwms/llm-edit/issues)); only the
ergonomic CLI subcommands above exist today, but `llm-edit apply
--json '<op>'` is always an escape hatch.

## What `.llm-edit/` contains

- `log.jsonl` — append-only record of every op an agent has recorded
  on this branch. Tracked in git. Carried forward through merges by
  the merge tool above.
- `conflicts.txt` — written by the merge tool only when a semantic
  conflict halts a merge. Not tracked in git (gitignored if it
  appears).
