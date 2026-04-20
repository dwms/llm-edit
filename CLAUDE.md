# llm-edit project instructions

This repo dogfoods its own structured-op merge tool. Edits are recorded as ops automatically by Claude Code hooks (`.claude/hooks/llm-edit-pre.sh`, `.claude/hooks/llm-edit-post.sh`) and by the git pre-commit hook — you don't need to call `llm-edit` subcommands by hand. Just use Edit/Write normally.

If a hook ever needs debugging, the subcommands are documented in `README.md`.

## Merging

Use `llm-edit git-merge BRANCH` instead of `git merge`. Use `llm-edit git-rebase ONTO` instead of `git rebase`. These can't be inferred — they have to be invoked explicitly.

## Building

Always `cargo build --release` before dogfood-merging. The post-merge hook auto-rebuilds if src/ changed, but manual builds may be needed on the initial branch. The Claude Code hooks also no-op silently until the binary is built.

## Testing

Run `cargo test` (16 integration tests) and `cargo run --release --bin bench` (128 fixtures, gap must be >= 0.90) before merging.
