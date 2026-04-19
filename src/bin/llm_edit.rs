// Recording CLI. An agent drives this in their worktree; each invocation
// performs an edit on local files and appends the corresponding op to
// `.llm-edit/log.jsonl`. Ergonomic subcommands for common ops; the
// universal `apply --json` escape hatch handles the rest.

use anyhow::Result;
use clap::{Parser, Subcommand};
use llm_edit::cli::{
    apply_op_json, merge_worktrees, record_delete_comment, run_git_merge, run_infer,
    run_merge_driver,
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
    /// Rename an identifier. With --scope module (default), renames in
    /// one file. With --scope project, renames across all source files
    /// (Rust, Python, JS/TS, HTML).
    RenameSymbol {
        #[arg(long)]
        file: String,
        old: String,
        new: String,
        /// "module" (default) = single file; "project" = all source files.
        #[arg(long, default_value = "module")]
        scope: String,
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
    /// Delete a comment inside a named function, identified by its text
    /// content (hashed to a stable node_id). Survives concurrent
    /// insert-before edits from another agent, which would otherwise
    /// drift a positional `{kind: "comment", index: N}` anchor.
    #[command(name = "delete-comment")]
    DeleteComment {
        #[arg(long)]
        file: String,
        #[arg(long = "fn")]
        fn_name: String,
        #[arg(long)]
        text: String,
    },
    /// Add a new function after a named sibling in the same module.
    #[command(name = "add-function")]
    AddFunction {
        /// File to insert into.
        #[arg(long)]
        file: String,
        /// Name of the existing function to insert after.
        #[arg(long)]
        after: String,
        /// Name of the new function.
        #[arg(long)]
        name: String,
        /// Full source text of the function to add (use \n for newlines).
        #[arg(long)]
        source: String,
    },
    /// Create a new file with the given content.
    #[command(name = "add-file")]
    AddFile {
        #[arg(long)]
        file: String,
        /// File content (use \n for newlines, or pipe from stdin via --content "$(cat path)").
        #[arg(long)]
        content: String,
    },
    /// Delete an entire file.
    #[command(name = "delete-file")]
    DeleteFile2 {
        #[arg(long)]
        file: String,
    },
    /// Rename / relocate a file.
    #[command(name = "move-file")]
    MoveFile {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Add or remove parameters from a function signature.
    #[command(name = "change-signature")]
    ChangeSignature {
        #[arg(long)]
        file: String,
        #[arg(long = "fn")]
        fn_name: String,
        /// Parameters to append (e.g. "verbose: bool"). Repeatable.
        #[arg(long = "add")]
        add_params: Vec<String>,
        /// Parameter names to remove. Repeatable.
        #[arg(long = "remove")]
        remove_params: Vec<String>,
    },
    /// Delete an entire function from a file.
    #[command(name = "delete-function")]
    DeleteFunction {
        #[arg(long)]
        file: String,
        /// Name of the function to delete.
        #[arg(long = "fn")]
        fn_name: String,
    },
    /// Delete an AST node identified by an anchor (raw JSON).
    #[command(name = "delete-node")]
    DeleteNode {
        /// Anchor JSON, e.g. '{"file":"x.py","path":["module","fn","helper"]}'.
        #[arg(long)]
        anchor: String,
    },
    /// Replace a substring within an anchored region (function body,
    /// element, etc.).
    #[command(name = "replace-in-anchor")]
    ReplaceInAnchor {
        #[arg(long)]
        file: String,
        /// Name of the function whose body to search.
        #[arg(long = "fn")]
        fn_name: String,
        /// Old text to find.
        #[arg(long)]
        old: String,
        /// New text to substitute.
        #[arg(long)]
        new: String,
    },
    /// Insert content after the first line matching a needle string.
    #[command(name = "insert-after")]
    InsertAfter {
        #[arg(long)]
        file: String,
        /// A substring that uniquely identifies the line to insert after.
        #[arg(long)]
        after_line: String,
        /// Source text to insert (use \n for newlines).
        #[arg(long)]
        content: String,
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
        /// Branch to rebase onto (omit when using --continue or --abort).
        onto: Option<String>,
        /// Resume a paused rebase whose conflict you've now resolved.
        #[arg(long, conflicts_with = "abort")]
        r#continue: bool,
        /// Abandon a paused rebase and restore the original branch.
        #[arg(long)]
        abort: bool,
        /// Open `$EDITOR` on the rebase todo so commits can be reordered,
        /// dropped, squashed, fixed up, or reworded before replay.
        #[arg(short = 'i', long)]
        interactive: bool,
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
        Cmd::GitRebase { onto, r#continue, abort, interactive } => {
            let code = if abort {
                llm_edit::cli::run_git_rebase_abort()?
            } else if r#continue {
                llm_edit::cli::run_git_rebase_continue()?
            } else {
                let onto = onto.ok_or_else(|| {
                    anyhow::anyhow!("git-rebase: <onto> required when not using --continue / --abort")
                })?;
                llm_edit::cli::run_git_rebase_ex(&onto, interactive)?
            };
            std::process::exit(code);
        }
        Cmd::DeleteComment { file, fn_name, text } => {
            return record_delete_comment(&cwd, &file, &fn_name, &text);
        }
        _ => {}
    }
    let json = match args.cmd {
        Cmd::Merge { .. }
        | Cmd::GitMerge { .. }
        | Cmd::MergeDriver { .. }
        | Cmd::Infer
        | Cmd::GitRebase { .. }
        | Cmd::DeleteComment { .. } => unreachable!(),
        Cmd::AddFile { file, content } => json!({
            "op": "add_file",
            "file": file,
            "content": content,
        })
        .to_string(),
        Cmd::DeleteFile2 { file } => json!({
            "op": "delete_file",
            "file": file,
        })
        .to_string(),
        Cmd::MoveFile { from, to } => json!({
            "op": "move_file",
            "from": from,
            "to": to,
        })
        .to_string(),
        Cmd::ChangeSignature { file, fn_name, add_params, remove_params } => json!({
            "op": "change_signature",
            "file": file,
            "fn_name": fn_name,
            "add_params": add_params,
            "remove_params": remove_params,
        })
        .to_string(),
        Cmd::AddFunction { file, after, name, source } => json!({
            "op": "add_function",
            "anchor": { "file": file, "path": ["module"] },
            "after": { "kind": "function_def", "name": after },
            "name": name,
            "source": source,
        })
        .to_string(),
        Cmd::DeleteFunction { file, fn_name } => json!({
            "op": "delete_function",
            "anchor": { "file": file, "path": ["module", {"kind": "function_def", "name": fn_name}] },
        })
        .to_string(),
        Cmd::DeleteNode { anchor } => {
            let anchor_val: serde_json::Value = serde_json::from_str(&anchor)
                .expect("--anchor must be valid JSON");
            json!({
                "op": "delete_node",
                "anchor": anchor_val,
            })
            .to_string()
        },
        Cmd::ReplaceInAnchor { file, fn_name, old, new } => json!({
            "op": "replace_in_anchor",
            "anchor": { "file": file, "path": ["module", {"kind": "function_def", "name": fn_name}] },
            "old": old,
            "new": new,
        })
        .to_string(),
        Cmd::InsertAfter { file, after_line, content } => json!({
            "op": "insert_after",
            "anchor": { "file": file, "after_line_contains": after_line },
            "content": content,
        })
        .to_string(),
        Cmd::Apply { json } => json,
        Cmd::RenameSymbol { file, old, new, scope } => json!({
            "op": "rename_symbol",
            "scope": scope,
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
