# llm-edit

Structured-op merge tool for multi-agent git workflows. Records source code edits as typed operations (rename, add function, delete file, etc.) and uses them to merge branches semantically — resolving conflicts that plain `git merge` cannot.

## The problem

When multiple LLM agents work in parallel git branches, merging their changes is expensive:
- Plain `git merge` produces text conflicts on structurally-unambiguous edits (two agents add different methods to the same impl block, two agents add different dependencies to Cargo.toml)
- Each conflict costs an LLM resolver ~40K tokens to read + rewrite an 80KB file
- With 3+ agents, costs compound linearly per conflict

## How it works

llm-edit is **event-sourced version control on top of git**:
1. Each edit is recorded as a structured op (rename_symbol, add_function, delete_file, etc.)
2. Ops are embedded in commit message bodies as fenced JSONL blocks
3. Git history IS the op log — no external state to sync
4. At merge time, the semantic merger compares ops instead of diffing text

When two branches both rename the same symbol to different names, llm-edit produces a one-line conflict report (`rename_symbol_mismatch`) instead of a full-file diff3 conflict. The recorded rename-conflict example uses roughly 800 times fewer tokens;
this is an illustrative estimate, not a general workload guarantee.

For edits without recorded ops, llm-edit falls back to AST-aware 3-way merge (splits files into named items, merges per-item) which handles most disjoint-edit cases that plain diff3 can't.

## Install

```bash
# Homebrew (macOS / Linux)
brew install dwms/tap/llm-edit

# …or clone and build from source (requires Rust toolchain)
git clone https://github.com/dwms/llm-edit.git
cd llm-edit
cargo install --locked --path . --bin llm-edit --bin lint_parse

# Initialize in your project
cd your-project
llm-edit init
```

The v0.1.0 prebuilt archives contain only `llm-edit`. Build from source for
the `lint_parse` helper used by the pre-commit hook. Future release archives
include both binaries.

`init` installs four git hooks (pre-commit, prepare-commit-msg, post-commit, post-merge) and adds the transient scratch file to `.gitignore`. No other workspace artifacts.

## Usage

### Record ops as you edit

```bash
# Rename a symbol across all source files
llm-edit rename-symbol --file src/lib.rs --scope project old_name new_name

# Add a function after an existing one
llm-edit add-function --file src/lib.rs --after existing_fn --name new_fn --source 'fn new_fn() { ... }'

# Add/remove function parameters
llm-edit change-signature --file src/lib.rs --fn process --add "verbose: bool" --remove debug

# File operations
llm-edit add-file --file src/new_module.rs --content '...'
llm-edit delete-file --file src/old_module.rs
llm-edit move-file --from src/old.rs --to src/new.rs

# Or just edit normally — the pre-commit hook auto-infers ops
vim src/lib.rs
git commit -m "refactor: extract helper"
# Hook runs `llm-edit infer`, detects the structural change, embeds ops in commit body
```

### Merge with semantic awareness

```bash
# Instead of `git merge feature-branch`:
llm-edit git-merge feature-branch

# Instead of `git rebase main`:
llm-edit git-rebase main
# Interactive rebase also supported:
llm-edit git-rebase main -i
```

### Claude Code integration

When Claude Code edits files in this repo, hooks in `.claude/hooks/` keep the op log live without the agent having to call subcommands explicitly:

| Hook | Event | What it does |
|---|---|---|
| `llm-edit-pre.sh` | `PreToolUse` on `Edit`/`Write` | Intercepts unambiguous intents — `Write` to a new path is routed through `add-file`; `Edit` with `replace_all: true` on a bare identifier is routed through `rename-symbol`. The agent's tool call is suppressed; the op is recorded with full intent. |
| `llm-edit-post.sh` | `PostToolUse` on `Edit`/`Write` | Runs `llm-edit infer` on the resulting working tree to backfill ops from the diff. Catches everything the pre hook didn't intercept. |
| `llm-edit-bash.sh` | `PreToolUse` on `Bash` | Blocks raw `git merge` / `git rebase` with a message redirecting to `llm-edit git-merge` / `llm-edit git-rebase`. Bypass with `LLM_EDIT_BYPASS=1` (e.g. for `git merge --abort`). |

Net effect: agents call `Edit`/`Write` and `git` normally; ops get recorded and merges go through the structured-op path automatically. The hooks no-op silently until `cargo build --release` produces the binary.

### All subcommands

```
llm-edit init                  Set up hooks + .gitignore in a git repo
llm-edit rename-symbol         Rename identifier (--scope module|project)
llm-edit rename-css-class      Rename CSS class across CSS + HTML + JSX
llm-edit add-function          Add a function after a named sibling
llm-edit delete-function       Delete a function by name
llm-edit change-signature      Add/remove function parameters
llm-edit add-file              Create a new file
llm-edit delete-file           Remove a file
llm-edit move-file             Rename/relocate a file
llm-edit replace-in-anchor     Replace text within a function body
llm-edit insert-after          Insert content after a matching line
llm-edit delete-node           Delete an AST node by anchor
llm-edit delete-comment        Delete a comment by content hash
llm-edit add-import            Add an import statement
llm-edit sort-block            Sort the import block alphabetically
llm-edit apply --json '...'    Apply any op as raw JSON
llm-edit infer                 Auto-detect ops from working tree changes
llm-edit git-merge BRANCH      Semantic merge (replaces git merge)
llm-edit git-rebase ONTO       Semantic rebase (replaces git rebase)
```

## Languages supported

AST-aware merging and parse-checking for:
- Python, JavaScript, TypeScript, JSX/TSX
- Rust
- CSS, HTML
- TOML (Cargo.toml dependency merges)

## How ops get into commits

The git hooks handle this automatically:

1. **pre-commit**: runs `llm-edit infer` to detect structural changes from your text edits
2. **prepare-commit-msg**: moves pending ops from the scratch file into the commit body as a fenced block
3. **post-commit**: clears the scratch file
4. **post-merge**: rebuilds the llm-edit binary if source changed (dogfood protection)

Ops in a commit body look like:
````
feat: add validation helper

```llm-edit-ops
{"op":"add_function","anchor":{"file":"src/lib.rs","path":["module"]},"after":{"kind":"function_def","name":"process"},"name":"validate","source":"fn validate() { ... }\n"}
```
````

The merger reads these via `git log --format=%B base..tip` — no files to sync, no state to lose.

## Token savings

The fixture suite measures merge behavior; token counts below include estimates
for larger files and are not a production workload benchmark. Run
`cargo run --locked --release --bin bench` to reproduce the merge results.

Current merge benchmark and illustrative token estimates:

| Metric | Value |
|---|---|
| Fixtures passing expected merge behavior | 128/128 |
| Fixtures passing with diff3 | 5/128 |
| Estimated tokens saved per fixture (small files) | ~160 |
| Estimated tokens saved for an 80KB file conflict | ~36,000 |
| Reduction for recorded rename-vs-rename | ~800x |

## License

MIT
