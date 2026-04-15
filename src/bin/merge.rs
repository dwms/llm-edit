// Merge two agents' op-logs on top of a common base and compare with
// `expected/` (or `conflict.txt` for negative cases). The actual merge
// pipeline lives in `llm_edit::merge_logic`; this binary just walks
// fixtures and prints results.

use anyhow::Result;
use llm_edit::merge_logic::check_case;
use std::path::PathBuf;
use walkdir::WalkDir;

fn main() -> Result<()> {
    let root = PathBuf::from("fixtures");
    let mut cases: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&root).min_depth(1).max_depth(4) {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            continue;
        }
        let a = entry.path().join("agent_a").join(".llm-edit").join("log.jsonl");
        let b = entry.path().join("agent_b").join(".llm-edit").join("log.jsonl");
        if a.exists() && b.exists() {
            cases.push(entry.path().to_path_buf());
        }
    }
    cases.sort();

    let mut total = 0usize;
    let mut ok = 0usize;
    for case in &cases {
        total += 1;
        let rel = case.strip_prefix("fixtures").unwrap_or(case).display().to_string();
        match check_case(case) {
            Ok(issues) if issues.is_empty() => {
                ok += 1;
                println!("PASS  {}", rel);
            }
            Ok(issues) => {
                println!("FAIL  {}", rel);
                for issue in issues {
                    println!("        - {}", issue);
                }
            }
            Err(e) => println!("ERROR {}  {:#}", case.display(), e),
        }
    }

    println!();
    println!("merger: {}/{} fixtures produced the expected outcome", ok, total);
    if ok != total {
        std::process::exit(1);
    }
    Ok(())
}
