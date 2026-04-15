//! Core recording-CLI logic: apply an op to a worktree and append it to
//! the worktree's `.llm-edit/log.jsonl`; and merge two worktrees' logs
//! together with persistence of the merged log to disk.

use anyhow::{Context, Result, anyhow};
use std::path::Path;

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
