// Recording CLI. An agent drives this in their worktree; each invocation
// performs an edit on local files and appends the corresponding op to
// `.llm-edit/log.jsonl`. Ergonomic subcommands for common ops; the
// universal `apply --json` escape hatch handles the rest.

use anyhow::Result;
use clap::{Parser, Subcommand};
use llm_edit::cli::{apply_op_json, merge_worktrees};
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "llm-edit", about = "Record structured edits so merges are trivial.")]
struct Args {
    /// Worktree directory to operate on (defaults to cwd).
    #[arg(long, global = true)]
    cwd: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Apply a raw op encoded as JSON. Use this for ops without
    /// dedicated subcommands.
    Apply {
        #[arg(long)]
        json: String,
    },
    /// Rename a module-scoped identifier across one file.
    RenameSymbol {
        #[arg(long)]
        file: String,
        old: String,
        new: String,
    },
    /// Rename a CSS class across the whole project (CSS rules + HTML
    /// `class=` attributes).
    RenameCssClass { old: String, new: String },
    /// Add `import <module>` in sorted position if the block is sorted,
    /// else append at the block's end.
    AddImport {
        #[arg(long)]
        file: String,
        module: String,
    },
    /// Sort the contiguous top-of-file import block alphabetically.
    SortBlock {
        #[arg(long)]
        file: String,
    },
    /// Merge two worktrees on top of a common base, persist merged
    /// source + merged op-log to `--output`. Use this in place of
    /// `git merge` / `diff3 -m` when both sides carry an `.llm-edit/`
    /// log — the persistence lets later merges reuse today's decisions.
    Merge {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        ours: PathBuf,
        #[arg(long)]
        theirs: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let cwd = args
        .cwd
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    // Merge is the one subcommand that isn't a single-op apply.
    if let Cmd::Merge { base, ours, theirs, output } = args.cmd {
        return merge_worktrees(&base, &ours, &theirs, &output);
    }
    let json = match args.cmd {
        Cmd::Merge { .. } => unreachable!(),
        Cmd::Apply { json } => json,
        Cmd::RenameSymbol { file, old, new } => json!({
            "op": "rename_symbol",
            "scope": "module",
            "file": file,
            "old": old,
            "new": new,
            "kinds": ["parameter", "identifier"],
        })
        .to_string(),
        Cmd::RenameCssClass { old, new } => json!({
            "op": "rename_css_class",
            "scope": "project",
            "old": old,
            "new": new,
        })
        .to_string(),
        Cmd::AddImport { file, module } => json!({
            "op": "add_import",
            "anchor": { "file": file },
            "module": module,
        })
        .to_string(),
        Cmd::SortBlock { file } => json!({
            "op": "sort_block",
            "anchor": { "file": file, "path": ["module", "imports"] },
            "key": "alphabetic",
        })
        .to_string(),
    };
    apply_op_json(&cwd, &json)
}
