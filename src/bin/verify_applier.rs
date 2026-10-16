// For each fixture that has `.llm-edit/log.jsonl` under agent_a or agent_b:
// 1. Load base/ into a tree.
// 2. Parse the log.
// 3. Apply it.
// 4. Compare every file against agent_*/ byte-for-byte.

use anyhow::{Context, Result, anyhow};
use llm_edit::{apply_log, load_tree, ops::parse_log};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let root = PathBuf::from("fixtures");
    let mut total = 0usize;
    let mut ok = 0usize;

    let mut cases: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&root).min_depth(1).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry.path().join("base").is_dir() {
            cases.push(entry.path().to_path_buf());
        }
    }
    cases.sort();

    for case in &cases {
        for agent in ["agent_a", "agent_b"] {
            let log_path = case.join(agent).join(".llm-edit").join("log.jsonl");
            if !log_path.exists() {
                continue;
            }
            total += 1;
            let verdict = verify_one(case, agent, &log_path);
            match verdict {
                Ok(diffs) if diffs.is_empty() => {
                    ok += 1;
                    println!("PASS  {} / {}", rel(case), agent);
                }
                Ok(diffs) => {
                    println!(
                        "FAIL  {} / {}  ({} file(s) differ)",
                        rel(case),
                        agent,
                        diffs.len()
                    );
                    for (p, reason) in diffs {
                        println!("        - {}: {}", p.display(), reason);
                    }
                }
                Err(e) => {
                    println!("ERROR {} / {}  {}", rel(case), agent, e);
                }
            }
        }
    }

    println!();
    println!(
        "applier verification: {}/{} agent logs reproduce their worktree exactly",
        ok, total
    );
    if ok != total {
        std::process::exit(1);
    }
    Ok(())
}

fn rel(p: &Path) -> String {
    p.strip_prefix("fixtures")
        .unwrap_or(p)
        .display()
        .to_string()
}

fn verify_one(case: &Path, agent: &str, log_path: &Path) -> Result<Vec<(PathBuf, String)>> {
    let base = case.join("base");
    let expected = case.join(agent);

    let mut tree = load_tree(&base).with_context(|| format!("load base of {}", case.display()))?;
    let log_text = std::fs::read_to_string(log_path)
        .with_context(|| format!("read {}", log_path.display()))?;
    let ops = parse_log(&log_text)?;
    apply_log(&mut tree, &ops)?;

    let expected_tree = load_tree(&expected)?;

    let mut diffs = Vec::new();
    let all: BTreeSet<&PathBuf> = tree.keys().chain(expected_tree.keys()).collect();
    for path in all {
        match (tree.get(path), expected_tree.get(path)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => diffs.push((path.clone(), "contents differ".to_string())),
            (Some(_), None) => diffs.push((path.clone(), "only in applied tree".to_string())),
            (None, Some(_)) => diffs.push((path.clone(), "only in expected tree".to_string())),
            (None, None) => unreachable!(),
        }
    }
    Ok(diffs)
}

#[allow(dead_code)]
fn _use_anyhow() -> Result<()> {
    Err(anyhow!("unused"))
}
