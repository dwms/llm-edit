# llm-edit project instructions

This repo dogfoods its own structured-op merge tool. Every edit should be recorded as an op so merges use the semantic path instead of text diff3.

## Recording edits

**Use llm-edit subcommands instead of raw Edit/Write tools whenever possible.** The pre-commit hook will auto-infer ops from text edits, but explicit recording is more reliable.

| Instead of... | Use... |
|---|---|
| Edit to rename a symbol | `llm-edit rename-symbol --file F --scope project old new` |
| Edit to change a function body | `llm-edit replace-in-anchor --file F --fn NAME --old '...' --new '...'` |
| Edit to add a function | `llm-edit add-function --file F --after SIBLING --name NAME --source '...'` |
| Edit to delete a function | `llm-edit delete-function --file F --fn NAME` |
| Edit to add/remove params | `llm-edit change-signature --file F --fn NAME --add 'param: Type'` |
| Write to create a new file | `llm-edit add-file --file F --content '...'` |
| Bash rm to delete a file | `llm-edit delete-file --file F` |
| Bash mv to move a file | `llm-edit move-file --from F --to G` |
| Edit to insert a line | `llm-edit insert-after --file F --after-line 'needle' --content '...'` |

For complex edits that don't fit any subcommand, use the Edit tool normally — the pre-commit hook runs `llm-edit infer` which catches most structural patterns.

## Merging

Use `llm-edit git-merge BRANCH` instead of `git merge`. Use `llm-edit git-rebase ONTO` instead of `git rebase`.

## Building

Always `cargo build --release` before dogfood-merging. The post-merge hook auto-rebuilds if src/ changed, but manual builds may be needed on the initial branch.

## Testing

Run `cargo test` (16 integration tests) and `cargo run --release --bin bench` (128 fixtures, gap must be >= 0.90) before merging.
