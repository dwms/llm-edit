//! Core recording-CLI logic: apply an op to a worktree and append it to
//! the worktree's `.llm-edit/log.jsonl`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::ops::Op;
use crate::{apply_log, load_tree};

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
