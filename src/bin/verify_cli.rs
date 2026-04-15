// Replay each hand-written fixture log through the CLI core
// (`apply_op_json`) — one op per invocation — and verify two things:
//
//   1. The resulting worktree matches agent_*/ byte-exactly.
//   2. The emitted .llm-edit/log.jsonl matches the hand-written log.
//
// This proves the CLI closes the loop: an agent-issued sequence of ops
// both mutates files AND produces the replayable log format the merger
// consumes.

use anyhow::{Context, Result};
use llm_edit::cli::apply_op_json;
use llm_edit::load_tree;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let fixtures = PathBuf::from("fixtures");
    let tmp_root = PathBuf::from("target/verify_cli");
    let _ = std::fs::remove_dir_all(&tmp_root);
    std::fs::create_dir_all(&tmp_root)?;

    let mut cases: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&fixtures).min_depth(1).max_depth(3) {
        let entry = entry?;
        if entry.file_type().is_dir() && entry.path().join("base").is_dir() {
            cases.push(entry.path().to_path_buf());
        }
    }
    cases.sort();

    let mut total = 0usize;
    let mut ok = 0usize;
    for case in &cases {
        for agent in ["agent_a", "agent_b"] {
            let log_path = case.join(agent).join(".llm-edit/log.jsonl");
            if !log_path.exists() {
                continue;
            }
            total += 1;
            let rel = case.strip_prefix(&fixtures).unwrap_or(case).display().to_string();
            match verify_one(case, agent, &tmp_root) {
                Ok(issues) if issues.is_empty() => {
                    ok += 1;
                    println!("PASS  {} / {}", rel, agent);
                }
                Ok(issues) => {
                    println!("FAIL  {} / {}", rel, agent);
                    for issue in issues {
                        println!("        - {}", issue);
                    }
                }
                Err(e) => println!("ERROR {} / {}  {:#}", rel, agent, e),
            }
        }
    }
    println!();
    println!("cli verification: {}/{} replays reproduce worktree + log", ok, total);
    if ok != total {
        std::process::exit(1);
    }
    Ok(())
}

fn verify_one(case: &Path, agent: &str, tmp_root: &Path) -> Result<Vec<String>> {
    let id = format!(
        "{}_{}",
        case.strip_prefix("fixtures")
            .unwrap_or(case)
            .display()
            .to_string()
            .replace('/', "_"),
        agent
    );
    let tmp = tmp_root.join(id);
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    // Copy base/ → tmp.
    let base = case.join("base");
    copy_dir_contents(&base, &tmp)?;

    // Replay log.
    let log_path = case.join(agent).join(".llm-edit/log.jsonl");
    let log_text = std::fs::read_to_string(&log_path)?;
    for (i, line) in log_text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        apply_op_json(&tmp, trimmed)
            .with_context(|| format!("replay line {}: {}", i + 1, trimmed))?;
    }

    // Compare worktree contents (excluding .llm-edit) with agent_*/.
    let mut issues = Vec::new();
    let produced = load_tree(&tmp)?;
    let expected = load_tree(&case.join(agent))?;
    let all: BTreeSet<&PathBuf> = produced.keys().chain(expected.keys()).collect();
    for path in all {
        match (produced.get(path), expected.get(path)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => issues.push(format!("{}: contents differ", path.display())),
            (Some(_), None) => issues.push(format!("{}: only in produced", path.display())),
            (None, Some(_)) => issues.push(format!("{}: only in expected", path.display())),
            (None, None) => unreachable!(),
        }
    }

    // Compare emitted log with hand-written log.
    let emitted = std::fs::read_to_string(tmp.join(".llm-edit/log.jsonl"))?;
    if emitted != log_text {
        issues.push("log.jsonl: differs from hand-written".to_string());
    }

    Ok(issues)
}

fn copy_dir_contents(from: &Path, to: &Path) -> Result<()> {
    for entry in WalkDir::new(from) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(from)?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}
