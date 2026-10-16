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

When two branches both rename the same symbol to different names, llm-edit
produces a structured conflict report (`rename_symbol_mismatch`). When a rename
and a new caller are compatible, it propagates the rename into the new code.
No LLM inference is used by the merge engine.

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

## Merge results and token savings

On the curated, recorded-operation fixtures, llm-edit produces the expected
result in **81/81 compatible single merges**, compared with **5/81 for the
per-file `diff3 -m` baseline**. That avoids a resolution step in 76 cases.
These fixtures target semantic merge challenges; they do not estimate the
success rate of ordinary merges in an arbitrary repository.

| Metric | Reproduced result |
| --- | ---: |
| Compatible single merges: llm-edit / diff3 | 81/81 / 5/81 |
| Additional cases resolved automatically | 76 (93.8% of this subset) |
| Estimated LLM repair tokens avoided across those cases | 12,218 |
| Estimated repair tokens avoided per additional resolution | 160 average |
| LLM tokens used by llm-edit's merge engine | 0 |

The token estimate counts text that a hypothetical LLM resolver would read and
rewrite, using **characters / 4**, with integer rounding per fixture. It is not
measured model usage or billing. It excludes prompts, retries, recording edits,
and human review. Smaller prompts or a patch-based resolver can reduce the
baseline cost. We do not claim a universal reduction factor.

Reproduce both reports from the repository root:

```sh
cargo run --locked --release --bin bench
cargo run --locked --release --bin token_savings
```

The full suite also checks 22 genuine conflicts and 25 chained merges; all
128 expected outcomes pass. See [benchmark methodology and results](BENCHMARK.md)
for the separate scoring rules and [raw token report](docs/benchmarks/token-savings.txt).

### Scenarios llm-edit resolves correctly that standard text merging does not

In each example below, **llm-edit automatically produces the correct merged
result**, matching the expected fixture output. Standard text merging with
`diff3 -m` and `git merge-file -p` either leaves a conflict or accepts an
incorrect result. These comparisons cover per-file text merging; other Git
merge strategies and third-party semantic tools were not evaluated.

#### 1. Rename a function while another branch adds a new caller

The project starts with a function named `square_root()`. One agent renames it
to `sqrt()` and updates the existing callers. At the same time, another agent
adds a new `magnitude()` function that still calls `square_root()`, because that
was the name on its branch.

**Standard text merging:** both tested commands produce a conflict. They compare
changed lines without knowing that the rename should also apply to the new
caller introduced on the other branch.

**llm-edit:** carries the recorded rename into the other agent's addition. The
merged project keeps the new function and uses `sqrt()` consistently:

```python
def sqrt(n):
    return n ** 0.5


def magnitude(v):
    return sqrt(v.x * v.x + v.y * v.y + v.z * v.z)
```

[Inspect the complete rename fixture, including both branches and the expected result.](fixtures/benchmark/rename-refactor-vs-new-usage/py-rename-fn-vs-new-call)

#### 2. Rename a CSS class while another branch adds HTML using the old name

One agent renames `.tag` to `.label` across three stylesheets: `main.css`,
`print.css`, and `dark.css`. Another agent adds this element to `page.html`:

```html
<span class="tag">Hello</span>
```

**Standard text merging:** both commands accept the HTML without a conflict,
but leave `class="tag"` unchanged. The merged stylesheets define `.label`, so
the new element no longer references the intended styles. A clean text merge
has produced an inconsistent result across files.

**llm-edit:** applies the recorded project-wide class rename to the new HTML as
well as the stylesheets. It preserves the element and its content, with the
correct class reference:

```html
<span class="label">Hello</span>
```

[Inspect the complete cross-file CSS/HTML fixture.](fixtures/benchmark/cross-file-ops/css-rename-3-css-files-vs-new-html)

#### 3. Sort an import block while another branch adds an import

The original Python file imports `zlib`, `base64`, and `pathlib` in that order.
One agent alphabetizes the block. Another agent appends `import csv` to the
original, unsorted block. Both changes are useful: keep the new dependency and
keep the imports sorted.

**Standard text merging:** both tested commands produce conflicts between the
reordered lines and the insertion, leaving someone to reconcile the block.

**llm-edit:** combines the recorded sort and insertion, preserving every import
exactly once in the expected order:

```python
import base64
import csv
import pathlib
import zlib
```

[Inspect the complete import-sorting fixture.](fixtures/benchmark/ordering-changes-vs-insert/py-sort-imports-vs-add-import-between)

For example, inspect the rename conflict locally (exit status 1 is expected):

```sh
case_dir=fixtures/benchmark/rename-refactor-vs-new-usage/py-rename-fn-vs-new-call
diff3 -m "$case_dir/agent_a/math.py" "$case_dir/base/math.py" "$case_dir/agent_b/math.py"
git merge-file -p "$case_dir/agent_a/math.py" "$case_dir/base/math.py" "$case_dir/agent_b/math.py"
# The expected semantic result is in "$case_dir/expected/math.py".
```

## License

MIT
