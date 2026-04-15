// Recording CLI. An agent drives this in their worktree; each invocation
// performs an edit on local files and appends the corresponding op to
// `.llm-edit/log.jsonl`. Ergonomic subcommands for common ops; the
// universal `apply --json` escape hatch handles the rest.

use anyhow::Result;
use clap::{Parser, Subcommand};
use llm_edit::cli::{
    apply_op_json, merge_worktrees, run_git_merge, run_git_rebase, run_infer, run_merge_driver,
};
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
    /// Git-integrated merge. Run from a clean working tree on the
    /// target branch. Reads both sides' trees + `.llm-edit/log.jsonl`
    /// from git's object store, merges via the structured-op merger,
    /// writes merged files and merged log to the working tree, and
    /// (by default) creates a merge commit with two parents.
    #[command(name = "git-merge")]
    GitMerge {
        branch: String,
        /// Skip the final `git commit`. Leaves the merged tree staged
        /// in the working directory for inspection.
        #[arg(long)]
        no_commit: bool,
    },
    /// Walk every changed file vs HEAD; for each, try to derive a
    /// structured op from the before/after pair. Currently detects
    /// single-identifier renames in py / js / jsx / mjs / cjs / ts /
    /// tsx / rs files. Inferred ops are appended to .llm-edit/log.jsonl;
    /// files on disk are not modified. Run AFTER editing, BEFORE
    /// committing, so the commit captures both source + log.
    Infer,
    /// Replay every commit on the current branch onto a new base,
    /// composing each commit's changes via the structured-op merger.
    /// Each commit becomes a single-parent commit on top of the new
    /// base; original commit shas are abandoned. On conflict, leaves
    /// HEAD detached at the partially-rebased state.
    #[command(name = "git-rebase")]
    GitRebase {
        onto: String,
    },
    /// (Experimental) Git merge-driver entry point. Intended for
    /// `.gitattributes merge=llm-edit`, but git sets `.git/MERGE_HEAD`
    /// only AFTER per-file drivers run, so this variant can't yet do
    /// cross-file propagation reliably. Prefer `git-merge`.
    #[command(name = "merge-driver")]
    MergeDriver {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        ours: PathBuf,
        #[arg(long)]
        theirs: PathBuf,
        #[arg(long)]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let cwd = args
        .cwd
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    // Non-apply subcommands that don't build a single op:
    match args.cmd {
        Cmd::Merge { base, ours, theirs, output } => {
            return merge_worktrees(&base, &ours, &theirs, &output);
        }
        Cmd::GitMerge { branch, no_commit } => {
            let code = run_git_merge(&branch, !no_commit)?;
            std::process::exit(code);
        }
        Cmd::MergeDriver { base, ours, theirs, path } => {
            let code = run_merge_driver(&base, &ours, &theirs, &path)?;
            std::process::exit(code);
        }
        Cmd::Infer => {
            let (inferred, skipped) = run_infer()?;
            println!("inferred {} op(s); {} file(s) had no detectable pattern", inferred, skipped);
            return Ok(());
        }
        Cmd::GitRebase { onto } => {
            let code = run_git_rebase(&onto)?;
            std::process::exit(code);
        }
        _ => {}
    }
    let json = match args.cmd {
        Cmd::Merge { .. }
        | Cmd::GitMerge { .. }
        | Cmd::MergeDriver { .. }
        | Cmd::Infer
        | Cmd::GitRebase { .. } => unreachable!(),
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
