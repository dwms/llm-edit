// Multi-merge benchmark harness. For each case under
// `fixtures/benchmark/multi-merge/`, runs two merges sequentially:
//
//   gen0 + gen1_left + gen1_right → gen1_state
//   gen1_state + gen2 → gen2_state
//
// Scored in two llm-edit configurations:
//
//   with-persistence:  the merged op-log from the first merge is used
//                      as "A's log" in the second merge. Any refactor
//                      from gen 1 (rename, extract, wrap) therefore
//                      rewrites gen 2's payloads before application.
//
//   without-persistence: the second merge starts from an empty log on
//                        the gen 1 side — the first merge's decisions
//                        do not propagate forward.
//
// Prints a table comparing both configs. Exits 0 always; this is a
// measurement tool, not a gate.

use anyhow::{Context, Result};
use llm_edit::merge_logic::{MergeResult, compare_tree, merge_trees_and_ops};
use llm_edit::ops::{Op, parse_log};
use llm_edit::load_tree;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let root = PathBuf::from("fixtures/benchmark/multi-merge");
    if !root.is_dir() {
        println!("(no multi-merge corpus at {})", root.display());
        return Ok(());
    }

    let mut cases: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&root).min_depth(1).max_depth(1) {
        let entry = entry?;
        if entry.file_type().is_dir() {
            cases.push(entry.path().to_path_buf());
        }
    }
    cases.sort();

    if cases.is_empty() {
        println!("(no multi-merge cases)");
        return Ok(());
    }

    println!(
        "{:<45}  {:<18}  {:<18}",
        "case", "without-persistence", "with-persistence"
    );
    println!("{:-<45}  {:-<18}  {:-<18}", "", "", "");
    let (mut with_ok, mut without_ok) = (0usize, 0usize);
    for case in &cases {
        let rel = case
            .strip_prefix(&root)
            .unwrap_or(case)
            .display()
            .to_string();
        let without = run_case(case, false).map(|ok| ok).unwrap_or(false);
        let with = run_case(case, true).map(|ok| ok).unwrap_or(false);
        println!(
            "{:<45}  {:<18}  {:<18}",
            rel,
            if without { "PASS" } else { "FAIL" },
            if with { "PASS" } else { "FAIL" },
        );
        if with {
            with_ok += 1;
        }
        if without {
            without_ok += 1;
        }
    }
    println!("{:-<45}  {:-<18}  {:-<18}", "", "", "");
    let total = cases.len();
    println!(
        "{:<45}  {}/{:<15}  {}/{}",
        "TOTAL", without_ok, total, with_ok, total
    );

    Ok(())
}

fn run_case(case: &Path, with_persistence: bool) -> Result<bool> {
    let base = load_tree(&case.join("gen0"))
        .with_context(|| format!("load gen0 of {}", case.display()))?;
    let log_a = read_log(&case.join("gen1_left.jsonl"))?;
    let log_b = read_log(&case.join("gen1_right.jsonl"))?;
    let log_c = read_log(&case.join("gen2.jsonl"))?;

    // First merge.
    let gen1 = match merge_trees_and_ops(base, &log_a, &log_b)? {
        MergeResult::Merged { tree, log } => (tree, log),
        MergeResult::Conflicts(_) => return Ok(false),
    };
    let (gen1_tree, gen1_log) = gen1;

    // Second merge. With persistence, A's log carries the gen-1 history
    // forward. Without persistence, A's log is empty.
    let effective_a_log: Vec<Op> = if with_persistence {
        gen1_log
    } else {
        Vec::new()
    };
    let gen2 = match merge_trees_and_ops(gen1_tree, &effective_a_log, &log_c)? {
        MergeResult::Merged { tree, .. } => tree,
        MergeResult::Conflicts(_) => return Ok(false),
    };

    let issues = compare_tree(&gen2, &case.join("expected"))?;
    Ok(issues.is_empty())
}

fn read_log(path: &Path) -> Result<Vec<Op>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    parse_log(&text)
}
