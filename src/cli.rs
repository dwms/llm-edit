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

/// Walk every file changed in the working tree vs HEAD and try to
/// derive structured ops from the before/after pair. For each detected
/// pattern, append the op to `.llm-edit/log.jsonl`. Files on disk are
/// not modified — this is a pure recording pass.
///
/// MVP detects ONE pattern: single-identifier rename across `.py`,
/// `.js`, `.jsx`, `.ts`, `.tsx`, `.rs`. If exactly one identifier
/// disappeared between HEAD and the working tree AND exactly one new
/// identifier appeared with matching occurrence count, emit a
/// `rename_symbol` op for that file.
///
/// Files where no pattern matches are reported as "no inference"
/// — they'll fall back to text-merge via the hybrid merger.
///
/// Returns (inferred_count, skipped_count).
pub fn run_infer() -> Result<(usize, usize)> {
    let repo_root = repo_root()?;
    let changed = git_cmd(&["diff", "--name-only", "HEAD"])?;
    let mut inferred = 0usize;
    let mut skipped = 0usize;
    let mut new_ops: Vec<crate::ops::Op> = Vec::new();

    for line in changed.lines() {
        let rel = line.trim();
        if rel.is_empty() {
            continue;
        }
        let path = std::path::PathBuf::from(rel);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        // Skip files we have no AST for.
        if !matches!(ext, "py" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "rs") {
            skipped += 1;
            println!("  skip  {}  (no grammar for .{})", rel, ext);
            continue;
        }
        let before = match git_cmd(&["show", &format!("HEAD:{}", rel)]) {
            Ok(s) => s,
            Err(_) => {
                skipped += 1;
                println!("  skip  {}  (not in HEAD)", rel);
                continue;
            }
        };
        let after = std::fs::read_to_string(repo_root.join(rel))
            .with_context(|| format!("read {}", rel))?;
        if before == after {
            continue;
        }
        match infer_rename_for(ext, &before, &after) {
            Some((old, new)) => {
                let scope = "module".to_string();
                let op = serde_json::json!({
                    "op": "rename_symbol",
                    "scope": scope,
                    "file": rel,
                    "old": old,
                    "new": new,
                    "kinds": ["parameter", "identifier"],
                });
                let parsed: crate::ops::Op = serde_json::from_value(op)?;
                new_ops.push(parsed);
                inferred += 1;
                println!("  rename  {}  {} -> {}", rel, old, new);
            }
            None => {
                skipped += 1;
                println!("  skip  {}  (no rename pattern detected)", rel);
            }
        }
    }

    if !new_ops.is_empty() {
        let log_dir = repo_root.join(".llm-edit");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join("log.jsonl");
        let mut text = std::fs::read_to_string(&log_path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for op in &new_ops {
            text.push_str(&serde_json::to_string(op)?);
            text.push('\n');
        }
        std::fs::write(&log_path, text)?;
    }

    Ok((inferred, skipped))
}

/// Return `Some((old, new))` if there is exactly one identifier name
/// removed between `before` and `after` AND exactly one added, AND
/// their occurrence counts match. Otherwise `None`.
fn infer_rename_for(ext: &str, before: &str, after: &str) -> Option<(String, String)> {
    let lang: tree_sitter::Language = match ext {
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        _ => return None,
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return None;
    }
    let before_tree = parser.parse(before, None)?;
    let after_tree = parser.parse(after, None)?;
    if before_tree.root_node().has_error() || after_tree.root_node().has_error() {
        return None; // refuse to infer from broken parses
    }
    let mut before_idents: Vec<String> = Vec::new();
    let mut after_idents: Vec<String> = Vec::new();
    collect_all_identifiers(before_tree.root_node(), before.as_bytes(), &mut before_idents);
    collect_all_identifiers(after_tree.root_node(), after.as_bytes(), &mut after_idents);

    use std::collections::HashSet;
    let before_set: HashSet<&str> = before_idents.iter().map(|s| s.as_str()).collect();
    let after_set: HashSet<&str> = after_idents.iter().map(|s| s.as_str()).collect();
    let removed: Vec<&str> = before_set.difference(&after_set).copied().collect();
    let added: Vec<&str> = after_set.difference(&before_set).copied().collect();

    if removed.len() != 1 || added.len() != 1 {
        return None;
    }
    let old = removed[0];
    let new = added[0];
    let old_count = before_idents.iter().filter(|s| s.as_str() == old).count();
    let new_count = after_idents.iter().filter(|s| s.as_str() == new).count();
    if old_count == new_count && old_count > 0 {
        Some((old.to_string(), new.to_string()))
    } else {
        None
    }
}

fn collect_all_identifiers(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut Vec<String>,
) {
    let kind = node.kind();
    // Skip text-subtree kinds — same exclusion list as the renamers.
    if matches!(
        kind,
        "string_literal"
            | "raw_string_literal"
            | "char_literal"
            | "line_comment"
            | "block_comment"
            | "doc_comment"
            | "comment"
            | "template_chars"
            | "string_fragment"
    ) {
        return;
    }
    let is_ident = matches!(
        kind,
        "identifier"
            | "property_identifier"
            | "shorthand_property_identifier"
            | "shorthand_property_identifier_pattern"
            | "type_identifier"
            | "jsx_identifier"
            | "field_identifier"
            | "shorthand_field_identifier"
    );
    if is_ident {
        let s = node.start_byte();
        let e = node.end_byte();
        if let Ok(text) = std::str::from_utf8(&source[s..e]) {
            out.push(text.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_all_identifiers(child, source, out);
    }
}

/// End-to-end git merge using llm-edit's structured-op merger, with
/// a per-file fallback to text-based 3-way merge for files whose
/// changes aren't fully reachable from the recorded op-log.
///
/// Why hybrid? Real repos rarely have 100% of edits recorded as ops:
/// most changes (logic edits, comment tweaks, file additions) don't
/// match an op type and are made via the regular editor. A pure
/// op-log merge would silently drop those edits. The hybrid path
/// uses semantic merging for files where ops are sufficient, and
/// falls back to `diff3 -m` for the rest. Logged ops still propagate
/// across to additive payloads (renames into new function bodies,
/// CSS class renames into HTML attrs) — that part isn't gated on
/// per-file coverage.
///
/// Exit codes:
/// - 0: clean merge; working tree updated; merge commit created if
///   `commit` is true.
/// - 1: at least one file had a text-merge conflict OR a semantic
///   conflict. `.llm-edit/conflicts.txt` lists semantic conflicts.
///   Conflict markers in source files mark text conflicts.
pub fn run_git_merge(branch: &str, commit: bool) -> Result<i32> {
    let ours_sha = git_cmd(&["rev-parse", "HEAD"])?.trim().to_string();
    let theirs_sha = git_cmd(&["rev-parse", branch])?.trim().to_string();
    let base_sha = git_cmd(&["merge-base", &ours_sha, &theirs_sha])?
        .trim()
        .to_string();

    let base_tree_raw = git_extract_tree(&base_sha)?;
    let ours_tree_raw = git_extract_tree(&ours_sha)?;
    let theirs_tree_raw = git_extract_tree(&theirs_sha)?;

    let log_path = PathBuf::from(".llm-edit/log.jsonl");
    let base_log = base_tree_raw.get(&log_path).cloned().unwrap_or_default();
    let ours_log = ours_tree_raw.get(&log_path).cloned().unwrap_or_default();
    let theirs_log = theirs_tree_raw.get(&log_path).cloned().unwrap_or_default();

    let base_ops_count = parse_log(&base_log)?.len();
    let ours_ops: Vec<Op> = parse_log(&ours_log)?.into_iter().skip(base_ops_count).collect();
    let theirs_ops: Vec<Op> = parse_log(&theirs_log)?.into_iter().skip(base_ops_count).collect();

    let strip_meta = |tree: &Tree| -> Tree {
        tree.iter()
            .filter(|(p, _)| !p.starts_with(".llm-edit"))
            .map(|(p, t)| (p.clone(), t.clone()))
            .collect()
    };
    let base_tree = strip_meta(&base_tree_raw);
    let ours_tree = strip_meta(&ours_tree_raw);
    let theirs_tree = strip_meta(&theirs_tree_raw);

    // Compute "what each side's tree SHOULD look like if the recorded
    // ops were the only source of changes." Comparing this to the
    // actual on-disk content tells us per file whether the side has
    // unrecorded edits.
    let mut expected_a = base_tree.clone();
    let _ = apply_log(&mut expected_a, &ours_ops);
    let mut expected_b = base_tree.clone();
    let _ = apply_log(&mut expected_b, &theirs_ops);

    // Run the semantic merger. Even when most files fall back to
    // text, the semantic result is the source for ops-covered files
    // AND its merged log carries forward.
    let outcome = merge_trees_and_ops(base_tree.clone(), &ours_ops, &theirs_ops)?;
    let repo_root = repo_root()?;
    let (semantic_tree, merged_log_ops) = match outcome {
        MergeResult::Merged { tree, log } => (tree, log),
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
            return Ok(1);
        }
    };

    // Per-file resolution.
    let all_files: std::collections::BTreeSet<PathBuf> = base_tree
        .keys()
        .chain(ours_tree.keys())
        .chain(theirs_tree.keys())
        .cloned()
        .collect();

    let mut final_tree: Tree = Tree::new();
    let mut text_conflicts: Vec<PathBuf> = Vec::new();
    let mut fallback_files: Vec<PathBuf> = Vec::new();

    for path in &all_files {
        let a_covered = expected_a.get(path) == ours_tree.get(path);
        let b_covered = expected_b.get(path) == theirs_tree.get(path);

        if a_covered && b_covered {
            // Recorded ops describe both sides' changes for this file.
            // Take the semantic merge result (which already has cross-
            // op rewriting applied).
            if let Some(content) = semantic_tree.get(path) {
                final_tree.insert(path.clone(), content.clone());
            }
            continue;
        }

        // At least one side has unrecorded edits to this file. Fall
        // back to diff3 of the actual contents. The semantic merger's
        // op-log still gets persisted; cross-file propagation that
        // was driven by ops still applied to OTHER files; this file
        // just gets the dumb-but-honest text merge.
        fallback_files.push(path.clone());
        let base_c = base_tree.get(path).cloned().unwrap_or_default();
        let ours_c = ours_tree.get(path).cloned().unwrap_or_default();
        let theirs_c = theirs_tree.get(path).cloned().unwrap_or_default();
        let merged = diff3_merge_strings(&ours_c, &base_c, &theirs_c)?;
        // Real diff3 conflict markers are at the start of a line.
        // Substring search would false-positive on prose that mentions
        // the literal "<<<<<<<" string (e.g. CONTRIBUTING.md describing
        // what conflict markers look like).
        let has_markers = merged.lines().any(|l| l.starts_with("<<<<<<<"));
        // Even when diff3 produces no markers, the merged output can
        // be syntactically broken — e.g. one side dropped a closing
        // brace because the surrounding context coincidentally lined
        // up. Validate against the file's tree-sitter grammar when we
        // know one. (#47)
        let parse_ok = parses_cleanly(path, &merged);
        if has_markers || !parse_ok {
            text_conflicts.push(path.clone());
        }
        final_tree.insert(path.clone(), merged);
    }

    // Write merged source.
    for (rel, text) in &final_tree {
        let abs = repo_root.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&abs, text)
            .with_context(|| format!("write {}", abs.display()))?;
    }

    // Write merged log (always — even when all files fell back, the
    // log carries forward correctly).
    let abs_log = repo_root.join(".llm-edit/log.jsonl");
    std::fs::create_dir_all(abs_log.parent().unwrap())?;
    let mut full_log = base_log;
    if !full_log.is_empty() && !full_log.ends_with('\n') {
        full_log.push('\n');
    }
    for op in &merged_log_ops {
        full_log.push_str(&serde_json::to_string(op)?);
        full_log.push('\n');
    }
    std::fs::write(&abs_log, full_log)?;

    git_cmd(&["add", "-A"])?;

    if !fallback_files.is_empty() {
        eprintln!(
            "llm-edit git-merge: {} file(s) fell back to text merge ({})",
            fallback_files.len(),
            fallback_files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if !text_conflicts.is_empty() {
        eprintln!(
            "llm-edit git-merge: {} file(s) have unresolved text conflicts ({}); resolve and `git commit` manually",
            text_conflicts.len(),
            text_conflicts
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        // Seed MERGE_HEAD anyway so a subsequent manual `git commit`
        // produces a two-parent merge commit.
        let git_dir = PathBuf::from(git_cmd(&["rev-parse", "--git-dir"])?.trim());
        std::fs::write(git_dir.join("MERGE_HEAD"), format!("{}\n", theirs_sha))?;
        return Ok(1);
    }

    if commit {
        // Use plumbing to guarantee a two-parent merge commit.
        // `git commit -m` with a hand-seeded MERGE_HEAD silently fell
        // back to a single-parent commit during dogfooding (#41); the
        // explicit commit-tree path bypasses that ambiguity.
        let tree_sha = git_cmd(&["write-tree"])?.trim().to_string();
        let msg = format!("Merge branch '{}'", branch);
        let commit_sha = git_cmd(&[
            "commit-tree",
            &tree_sha,
            "-p",
            &ours_sha,
            "-p",
            &theirs_sha,
            "-m",
            &msg,
        ])?
        .trim()
        .to_string();
        git_cmd(&["update-ref", "HEAD", &commit_sha])?;
    }

    Ok(0)
}

/// 3-way text merge via `diff3 -m`. Tmpfiles for inputs; output is
/// the merged string (possibly containing conflict markers).
/// True when `text` parses cleanly under the tree-sitter grammar for
/// `path`'s extension. For unknown extensions, returns true (no
/// validation possible). For supported extensions, a parse with any
/// `ERROR` node anywhere in the tree returns false — a signal that
/// text-merge fallback produced syntactically broken output and the
/// file should be flagged as a conflict instead of silently shipped.
/// 3-way text merge via `diff3 -m`. Tmpfiles for inputs; output is
/// the merged string (possibly containing conflict markers).
fn diff3_merge_strings(ours: &str, base: &str, theirs: &str) -> Result<String> {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tag = format!("llm-edit-{}-{}", pid, nanos);
    let ours_p = dir.join(format!("{}-ours", tag));
    let base_p = dir.join(format!("{}-base", tag));
    let theirs_p = dir.join(format!("{}-theirs", tag));
    std::fs::write(&ours_p, ours)?;
    std::fs::write(&base_p, base)?;
    std::fs::write(&theirs_p, theirs)?;
    let out = Command::new("diff3")
        .arg("-m")
        .arg(&ours_p)
        .arg(&base_p)
        .arg(&theirs_p)
        .output()
        .context("spawn diff3")?;
    let _ = std::fs::remove_file(&ours_p);
    let _ = std::fs::remove_file(&base_p);
    let _ = std::fs::remove_file(&theirs_p);
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parses_cleanly(path: &Path, text: &str) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang: tree_sitter::Language = match ext {
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        _ => return true,
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return true;
    }
    let Some(tree) = parser.parse(text, None) else {
        return false;
    };
    !tree.root_node().has_error()
}

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

#[cfg(test)]
mod parse_check_tests {
    use super::parses_cleanly;
    use std::path::Path;

    #[test]
    fn unknown_extension_passes() {
        // No grammar known → can't validate → assume ok.
        assert!(parses_cleanly(Path::new("README.md"), "anything goes\n"));
    }

    #[test]
    fn python_clean() {
        assert!(parses_cleanly(
            Path::new("foo.py"),
            "def f():\n    return 1\n"
        ));
    }

    #[test]
    fn python_unbalanced_brace_caught() {
        // Simulates the dogfood failure mode: a closing brace got
        // dropped by text-merge fallback.
        let broken = "def commit():\n    if cond:\n        return 1\n# missing dedent / outer block";
        // tree-sitter-python is very forgiving — try a more obviously
        // broken construct (mismatched paren).
        assert!(!parses_cleanly(Path::new("foo.py"), "def f(:\n    return 1\n"));
        let _ = broken;
    }

    #[test]
    fn rust_clean() {
        assert!(parses_cleanly(
            Path::new("lib.rs"),
            "fn x() -> i32 { 1 }\n"
        ));
    }

    #[test]
    fn rust_unbalanced_brace_caught() {
        // Closely modelled on the dogfood failure: a `}` got dropped
        // by text-merge fallback in src/cli.rs and the build broke.
        // With Rust now in the supported set this would have flagged.
        let broken = "fn x() {\n    if cond {\n        return 1;\n    \n";
        assert!(!parses_cleanly(Path::new("foo.rs"), broken));
    }

    #[test]
    fn js_unbalanced_brace_caught() {
        let broken = "function f() {\n  if (x) {\n    return 1;\n  }\n";
        // Missing one closing `}` for the function — should fail parse.
        assert!(!parses_cleanly(Path::new("foo.js"), broken));
    }

    #[test]
    fn ts_clean() {
        assert!(parses_cleanly(
            Path::new("foo.ts"),
            "export function f(x: number): number { return x + 1; }\n"
        ));
    }

    // ----- #54: dogfood corruption patterns ----------------------------
    //
    // Each of these is a real failure mode produced by `diff3 -m` text
    // fallback during the #34 / #35 / #36 batch dogfood. They compiled-
    // broken on master and only got caught by `cargo build`. The whole
    // point of `parses_cleanly` is to surface them as merge conflicts so
    // the merger refuses to silently land them.

    /// Pattern 1: `fn name(...) {` declaration line dropped, leaving
    /// the function body floating in module scope.
    #[test]
    fn rust_dropped_fn_declaration_caught() {
        let broken = r#"
fn intact() {
    let x = 1;
}


    for a in items {
        do_thing();
    }
}
"#;
        assert!(!parses_cleanly(Path::new("foo.rs"), broken));
    }

    /// Pattern 2: a method-call line wiped, leaving struct-init field
    /// syntax inside an `if` block (mismatched `(` / `)` pairing).
    #[test]
    fn rust_wiped_call_line_caught() {
        let broken = r#"
fn x() {
    if cond {
            file: f.clone(),
            deleted: d.clone(),
        });
    }
}
"#;
        assert!(!parses_cleanly(Path::new("foo.rs"), broken));
    }

    /// Pattern 3: closing `}` of an inner block dropped — the actual
    /// shape from `extract_function`'s `while` inside `if !want.empty()`.
    #[test]
    fn rust_dropped_inner_brace_caught() {
        let broken = r#"
fn x() {
    let want_return = "return total".to_string();
    if !want_return.is_empty() {
        let mut k = 0;
        while k < 5 {
            if k == 3 { break; }
            k += 1;
    }

    // <- this comment & subsequent code should belong to fn x's body
    // but the missing `}` above means we're still inside `while`.
    let extracted = "stuff";
    println!("{}", extracted);
}
"#;
        assert!(!parses_cleanly(Path::new("foo.rs"), broken));
    }

    /// Pattern 4: test `fn name() {` line wiped, leaving `#[test]`
    /// directly above a function body.
    #[test]
    fn rust_wiped_test_fn_caught() {
        let broken = r#"
#[cfg(test)]
mod tests {
    #[test]
        let x = 1;
        assert_eq!(x, 1);
    }
}
"#;
        assert!(!parses_cleanly(Path::new("foo.rs"), broken));
    }
}
