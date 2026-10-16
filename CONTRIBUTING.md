# Contributing — using llm-edit on llm-edit

This repo dogfoods its own merge tool. The convention is small.

## Merging PRs

Use `llm-edit git-merge <branch>` instead of `gh pr merge` for any PR
that should land on `master`. From a clean checkout of `master`:

```sh
git fetch && git checkout master && git pull
cargo build --release --bin llm-edit   # see "rebuild before dogfood" below
target/release/llm-edit git-merge feat/issue-NN-foo
git push
```

### Rebuild before dogfood

Always rebuild the binary before running `git-merge` on a fresh
master pull. The merger's text-fallback uses tree-sitter to flag
syntactically-broken merge results — but only for grammars that
were compiled into your binary at the time. Running a stale binary
that lacks (e.g.) Rust grammar against a master that's gained Rust
detection means corruption will silently land. This actually
happened during the `#34/#35/#36` dogfood batch — the broken merges
on `archive/broken-master-da7bf02` were the result.

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

## Validation

Run these before submitting changes:

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo run --locked --release --bin bench
```

The benchmark command enforces its minimum improvement threshold. Keep the
fixture directories intact: they contain intentional base/left/right/expected
snapshots, including examples that are not standalone compilable projects.

## What `.llm-edit/` contains

- `log.jsonl` is transient scratch space for pending operations, ignored by Git.
  The commit-message hook embeds operations in commit bodies, then clears it.
  Historical fixtures may contain tracked logs for compatibility tests.
- `conflicts.txt` is an ignored report written when a semantic conflict stops a merge.

## Repository layout

- `src/`: merge engine, operation handling, and command-line tools.
- `tests/` and fixture directories: integration tests and reproducible merge cases.
- `hooks/` and `.claude/hooks/`: Git and optional editor integrations.
- `homebrew/`: release formula template; the published tap is maintained separately.
