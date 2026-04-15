//! Core recording-CLI logic: apply an op to a worktree and append it to
//! the worktree's `.llm-edit/log.jsonl`; and merge two worktrees' logs
//! together with persistence of the merged log to disk.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::merge_logic::{MergeResult, merge_trees_and_ops};
use crate::ops::{Op, parse_log};
use crate::{Tree, apply_log, load_tree};

/// Apply a single op (parsed from JSON) against `cwd`, write changed files
/// back to disk, and append the op to `cwd/.llm-edit/log.jsonl`.
pub fn apply_op_json(cwd: &Path, op_json: &str) -> Result<()> {
    let op: Op = serde_json::from_str(op_json)
        .with_context(|| format!("parse op JSON: {}", op_json))?;
    let mut tree = load_tree(cwd)?;
    let before = tree.clone();
    apply_log(&mut tree, &[op])?;
    // Write back only files that changed.
    for (rel, text) in &tree {
        if before.get(rel) != Some(text) {
            let abs = cwd.join(rel);
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&abs, text)
                .with_context(|| format!("write {}", abs.display()))?;
        }
    }
    // Append to log.
    append_op_to_log(cwd, op_json)?;
    Ok(())
}

/// Merge two worktrees on top of `base` and write merged source + log
/// to `output`. On conflict, writes `output/.llm-edit/conflicts.txt` and
/// returns an error.
pub fn merge_worktrees(base: &Path, ours: &Path, theirs: &Path, output: &Path) -> Result<()> {
    let base_tree = load_tree(base).with_context(|| format!("load base {}", base.display()))?;
    let ops_a = read_agent_log(ours)?;
    let ops_b = read_agent_log(theirs)?;
    match merge_trees_and_ops(base_tree, &ops_a, &ops_b)? {
        MergeResult::Merged { tree, log } => write_output(output, &tree, &log),
        MergeResult::Conflicts(reports) => {
            std::fs::create_dir_all(output.join(".llm-edit"))?;
            let mut body = String::new();
            for r in &reports {
                body.push_str(&serde_json::to_string(r)?);
                body.push('\n');
            }
            std::fs::write(output.join(".llm-edit/conflicts.txt"), body)?;
            Err(anyhow!("semantic merge conflict: {} report(s)", reports.len()))
        }
    }
}

fn read_agent_log(dir: &Path) -> Result<Vec<Op>> {
    let path = dir.join(".llm-edit/log.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    parse_log(&text)
}

fn write_output(output: &Path, tree: &Tree, log: &[Op]) -> Result<()> {
    std::fs::create_dir_all(output)?;
    for (rel, text) in tree {
        let abs = output.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs, text)
            .with_context(|| format!("write {}", abs.display()))?;
    }
    let log_dir = output.join(".llm-edit");
    std::fs::create_dir_all(&log_dir)?;
    let mut body = String::new();
    for op in log {
        body.push_str(&serde_json::to_string(op)?);
        body.push('\n');
    }
    std::fs::write(log_dir.join("log.jsonl"), body)?;
    Ok(())
}

/// End-to-end git merge using llm-edit's structured-op merger. The user
/// runs `llm-edit git-merge <branch>` from a clean working tree on the
/// target branch; this function computes the merge-base, extracts both
/// sides' trees (source + `.llm-edit/log.jsonl`) from git's object
/// store, runs `merge_trees_and_ops`, and writes the merged source +
/// merged log into the working tree. The caller creates the merge
/// commit separately (or passes `--commit` / `--no-commit` upstream).
///
/// Why not a git `merge=llm-edit` driver? Git invokes per-file drivers
/// BEFORE setting `.git/MERGE_HEAD`, so a driver can't ask git for the
/// "theirs" ref symbolically. A whole-tree custom command sidesteps
/// that limitation and also gets cross-file propagation for free.
pub fn run_git_merge(branch: &str, commit: bool) -> Result<i32> {
    let ours_sha = git_cmd(&["rev-parse", "HEAD"])?.trim().to_string();
    let theirs_sha = git_cmd(&["rev-parse", branch])?.trim().to_string();
    let base_sha = git_cmd(&["merge-base", &ours_sha, &theirs_sha])?
        .trim()
        .to_string();

    let base_tree = git_extract_tree(&base_sha)?;
    let ours_tree = git_extract_tree(&ours_sha)?;
    let theirs_tree = git_extract_tree(&theirs_sha)?;

    let log_path = PathBuf::from(".llm-edit/log.jsonl");
    let base_log = base_tree.get(&log_path).cloned().unwrap_or_default();
    let ours_log = ours_tree.get(&log_path).cloned().unwrap_or_default();
    let theirs_log = theirs_tree.get(&log_path).cloned().unwrap_or_default();

    let base_ops_count = parse_log(&base_log)?.len();
    let ours_ops: Vec<Op> = parse_log(&ours_log)?.into_iter().skip(base_ops_count).collect();
    let theirs_ops: Vec<Op> = parse_log(&theirs_log)?.into_iter().skip(base_ops_count).collect();

    // Feed only source to the merger; metadata stays in the log.
    let base_source: Tree = base_tree
        .into_iter()
        .filter(|(p, _)| !p.starts_with(".llm-edit"))
        .collect();

    let outcome = merge_trees_and_ops(base_source, &ours_ops, &theirs_ops)?;
    let repo_root = repo_root()?;

    match outcome {
        MergeResult::Merged { tree, log } => {
            for (rel, text) in &tree {
                let abs = repo_root.join(rel);
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&abs, text)
                    .with_context(|| format!("write {}", abs.display()))?;
            }
            let abs_log = repo_root.join(".llm-edit/log.jsonl");
            std::fs::create_dir_all(abs_log.parent().unwrap())?;
            let mut full_log = base_log;
            if !full_log.is_empty() && !full_log.ends_with('\n') {
                full_log.push('\n');
            }
            for op in &log {
                full_log.push_str(&serde_json::to_string(op)?);
                full_log.push('\n');
            }
            std::fs::write(&abs_log, full_log)?;

            // Stage everything and optionally commit as a merge. To get
            // a two-parent commit without spelling out `git commit-tree`,
            // seed `.git/MERGE_HEAD` with the theirs sha — `git commit`
            // then treats the next commit as a merge.
            git_cmd(&["add", "-A"])?;
            if commit {
                let git_dir = PathBuf::from(git_cmd(&["rev-parse", "--git-dir"])?.trim());
                std::fs::write(git_dir.join("MERGE_HEAD"), format!("{}\n", theirs_sha))?;
                let msg = format!("Merge branch '{}'", branch);
                git_cmd(&["commit", "-q", "-m", &msg])?;
            }
            Ok(0)
        }
        MergeResult::Conflicts(reports) => {
            let conflict_dir = repo_root.join(".llm-edit");
            std::fs::create_dir_all(&conflict_dir)?;
            let mut body = String::new();
            for r in &reports {
                body.push_str(&serde_json::to_string(r)?);
                body.push('\n');
            }
            std::fs::write(conflict_dir.join("conflicts.txt"), body)?;
            eprintln!(
                "llm-edit git-merge: {} semantic conflict(s); see .llm-edit/conflicts.txt",
                reports.len()
            );
            Ok(1)
        }
    }
}

/// Git merge-driver entry point. Invoked by git once per file covered by
/// a `.gitattributes` `merge=llm-edit` rule. Arguments mirror the tokens
/// git substitutes: %O/%A/%B/%P.
///
/// Rather than merging just this one file, we load the whole tree on
/// each side from git's object store (at `HEAD`, `MERGE_HEAD`, and the
/// merge-base), run `merge_trees_and_ops`, then write the requested
/// file's merged content back to `ours_path` and the merged op-log back
/// to `.llm-edit/log.jsonl` inside the working tree. The whole-tree
/// merge is what makes cross-file rewrites (class renames that touch
/// HTML + CSS, project-scoped symbol renames) land correctly.
///
/// Performance note: each of git's per-file invocations redoes the
/// whole-tree merge. That's wasteful but correct; a deterministic
/// cache keyed on the three SHAs would be the v2 improvement.
///
/// Exit codes follow git's merge-driver contract:
/// - 0: clean merge; `ours_path` now contains merged content.
/// - 1: conflict; `ours_path` unchanged and a `.llm-edit/conflicts.txt`
///   is written at the repo root for human attention.
pub fn run_merge_driver(
    _base_path: &Path,
    ours_path: &Path,
    _theirs_path: &Path,
    rel_path: &Path,
) -> Result<i32> {
    // Resolve refs via git. We need the merge-base for `base`, HEAD for
    // `ours`, and MERGE_HEAD for `theirs`. During `git merge`, git sets
    // `.git/MERGE_HEAD` before invoking per-file drivers, but on some
    // platforms / git versions `rev-parse MERGE_HEAD` isn't reliable at
    // this point — falling back to reading `.git/MERGE_HEAD` directly
    // is more robust.
    let ours_sha = git_cmd(&["rev-parse", "HEAD"])?.trim().to_string();
    let theirs_sha = read_merge_head()?;
    let base_sha = git_cmd(&["merge-base", &ours_sha, &theirs_sha])?
        .trim()
        .to_string();

    let base_tree = git_extract_tree(&base_sha)?;
    let ours_tree = git_extract_tree(&ours_sha)?;
    let theirs_tree = git_extract_tree(&theirs_sha)?;

    let log_path = PathBuf::from(".llm-edit/log.jsonl");
    let base_log = base_tree.get(&log_path).cloned().unwrap_or_default();
    let ours_log = ours_tree.get(&log_path).cloned().unwrap_or_default();
    let theirs_log = theirs_tree.get(&log_path).cloned().unwrap_or_default();

    let base_ops_count = parse_log(&base_log)?.len();
    let ours_ops: Vec<Op> = parse_log(&ours_log)?.into_iter().skip(base_ops_count).collect();
    let theirs_ops: Vec<Op> = parse_log(&theirs_log)?.into_iter().skip(base_ops_count).collect();

    // Strip the log from the tree passed to the merger — the merger
    // treats `.llm-edit/` as metadata, not merge input.
    let base_tree_source: Tree = base_tree
        .into_iter()
        .filter(|(p, _)| !p.starts_with(".llm-edit"))
        .collect();

    match merge_trees_and_ops(base_tree_source, &ours_ops, &theirs_ops)? {
        MergeResult::Merged { tree, log } => {
            // Write the requested file's merged content to %A.
            let merged = tree
                .get(rel_path)
                .ok_or_else(|| anyhow!(
                    "merged tree missing {} (is the file covered by both sides?)",
                    rel_path.display()
                ))?;
            std::fs::write(ours_path, merged)
                .with_context(|| format!("write {}", ours_path.display()))?;

            // Persist merged log to the working tree so the next merge
            // can reuse it. Merged log = base log + new ops produced by
            // this merge.
            let repo_root = repo_root()?;
            let abs_log = repo_root.join(".llm-edit/log.jsonl");
            std::fs::create_dir_all(abs_log.parent().unwrap())?;
            let mut full_log = base_log;
            if !full_log.is_empty() && !full_log.ends_with('\n') {
                full_log.push('\n');
            }
            for op in &log {
                full_log.push_str(&serde_json::to_string(op)?);
                full_log.push('\n');
            }
            std::fs::write(&abs_log, full_log)?;
            Ok(0)
        }
        MergeResult::Conflicts(reports) => {
            // Leave `ours_path` untouched (git will see no update and
            // mark the file as conflicted). Write conflicts report.
            let repo_root = repo_root()?;
            let conflict_dir = repo_root.join(".llm-edit");
            std::fs::create_dir_all(&conflict_dir)?;
            let mut body = String::new();
            for r in &reports {
                body.push_str(&serde_json::to_string(r)?);
                body.push('\n');
            }
            std::fs::write(conflict_dir.join("conflicts.txt"), body)?;
            eprintln!("llm-edit merge-driver: {} semantic conflict(s); see .llm-edit/conflicts.txt", reports.len());
            Ok(1)
        }
    }
}

fn git_cmd(args: &[&str]) -> Result<String> {
    let out = Command::new("git").args(args).output()
        .with_context(|| format!("spawn git {}", args.join(" ")))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8(out.stdout).unwrap_or_default())
}

/// Read the first ref from `.git/MERGE_HEAD` during an active merge.
/// Falls back to `git rev-parse MERGE_HEAD` if the file isn't present.
fn read_merge_head() -> Result<String> {
    let git_dir = git_cmd(&["rev-parse", "--git-dir"])?;
    let mh = PathBuf::from(git_dir.trim()).join("MERGE_HEAD");
    if let Ok(text) = std::fs::read_to_string(&mh) {
        if let Some(first) = text.lines().next() {
            return Ok(first.trim().to_string());
        }
    }
    Ok(git_cmd(&["rev-parse", "MERGE_HEAD"])?.trim().to_string())
}

fn repo_root() -> Result<PathBuf> {
    let s = git_cmd(&["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(s.trim()))
}

/// Extract every file at a given git ref into an in-memory Tree.
fn git_extract_tree(sha: &str) -> Result<Tree> {
    let listing = git_cmd(&["ls-tree", "-r", "--name-only", sha])?;
    let mut tree = Tree::new();
    for line in listing.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        let content = git_cmd(&["show", &format!("{}:{}", sha, path)])?;
        tree.insert(PathBuf::from(path), content);
    }
    Ok(tree)
}

fn append_op_to_log(cwd: &Path, op_json: &str) -> Result<()> {
    let dir = cwd.join(".llm-edit");
    std::fs::create_dir_all(&dir)?;
    let log_path = dir.join("log.jsonl");
    let mut text = std::fs::read_to_string(&log_path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let trimmed = op_json.trim();
    text.push_str(trimmed);
    text.push('\n');
    std::fs::write(&log_path, text)?;
    Ok(())
}
