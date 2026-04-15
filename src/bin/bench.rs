// Benchmark harness: run every case under `fixtures/benchmark/` through
// both diff3 and llm-edit, bucket by top-level directory, print a table,
// and exit 0 iff `(llm_edit_pct - diff3_pct) >= 0.90`.
//
// Scoring rules (see BENCHMARK.md):
//
// Single-merge positive (has `expected/`):
//   - llm-edit CORRECT  iff merged tree equals `expected/` byte-exactly.
//   - diff3    CORRECT  iff every file's `diff3 -m A base B` exits 0,
//                       produces no <<<<<<< markers, and matches expected.
//
// Single-merge negative (has `conflict.txt`):
//   - llm-edit CORRECT  iff emitted Conflicts matches conflict.txt
//                       (order-independent JSON compare).
//   - diff3    CORRECT  iff at least one file has <<<<<<< markers
//                       (surrogate for "refused to silently resolve").
//                       Silent-clean merges on a true-conflict case =
//                       WRONG.
//
// Multi-merge (has `gen0/` + `gen1_left.jsonl` + `gen2.jsonl`):
//   - llm-edit CORRECT  iff the persistence-enabled chained merge
//                       produces a tree equal to `expected/` byte-exactly.
//   - diff3    CORRECT  always false. Diff3 has no clean chained-merge
//                       analog; scoring it as 0 is the intended outcome
//                       for this bucket (and why persistence earns a
//                       place in the bench at all).

use anyhow::{Context, Result, anyhow};
use llm_edit::load_tree;
use llm_edit::merge_logic::{MergeResult, compare_conflicts, compare_tree, merge_case, merge_trees_and_ops};
use llm_edit::ops::{Op, parse_log};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use walkdir::WalkDir;

#[derive(Default, Debug, Clone, Copy)]
pub struct Tally {
    pub diff3_correct: usize,
    pub llm_correct: usize,
    pub total: usize,
}

/// Result of running the harness: per-bucket tallies, grand total, and
/// whether the 80-pt gap was met. Exposed for integration tests.
pub struct BenchReport {
    pub buckets: BTreeMap<String, Tally>,
    pub total: Tally,
    pub pass: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(pass) => {
            if pass {
                ExitCode::from(0)
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("bench: {:#}", e);
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    // Optional CLI arg overrides the default corpus root (used by tests).
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/benchmark"));
    match run_at(&root)? {
        Some(rep) => Ok(rep.pass),
        None => Ok(true), // empty corpus: exit 0
    }
}

/// Core of the harness; parameterized by root so tests can point at a
/// synthetic corpus in a tempdir. Returns `None` iff the corpus is empty
/// (the empty-corpus message has already been printed).
pub fn run_at(root: &Path) -> Result<Option<BenchReport>> {
    let cases = discover_cases(root);
    if cases.is_empty() {
        println!(
            "(no benchmark corpus yet; build fixtures/benchmark/ before measuring)"
        );
        return Ok(None);
    }

    let mut buckets: BTreeMap<String, Tally> = BTreeMap::new();
    let mut total = Tally::default();

    for case in &cases {
        let bucket = bucket_of(root, case).unwrap_or_else(|| "<unbucketed>".to_string());
        let (d3, llm) = score_case(case)?;
        let entry = buckets.entry(bucket).or_default();
        entry.total += 1;
        total.total += 1;
        if d3 {
            entry.diff3_correct += 1;
            total.diff3_correct += 1;
        }
        if llm {
            entry.llm_correct += 1;
            total.llm_correct += 1;
        }
    }

    let pass = print_table(&buckets, &total)?;
    Ok(Some(BenchReport {
        buckets,
        total,
        pass,
    }))
}

fn discover_cases(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    for entry in WalkDir::new(root).min_depth(1).max_depth(6) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir() {
            continue;
        }
        let p = entry.path();
        let has_base = p.join("base").is_dir();
        let has_a = p.join("agent_a").is_dir();
        let has_b = p.join("agent_b").is_dir();
        let has_expected = p.join("expected").is_dir();
        let has_conflict = p.join("conflict.txt").is_file();
        if has_base && has_a && has_b && (has_expected || has_conflict) {
            out.push(p.to_path_buf());
            continue;
        }
        // Multi-merge shape: gen0/ + gen1_*.jsonl + gen2.jsonl + expected/.
        let has_gen0 = p.join("gen0").is_dir();
        let has_gen1l = p.join("gen1_left.jsonl").is_file();
        let has_gen2 = p.join("gen2.jsonl").is_file();
        if has_gen0 && has_gen1l && has_gen2 && has_expected {
            out.push(p.to_path_buf());
        }
    }
    out.sort();
    out
}

fn is_multi_merge_case(case: &Path) -> bool {
    case.join("gen0").is_dir()
        && case.join("gen1_left.jsonl").is_file()
        && case.join("gen2.jsonl").is_file()
}

fn bucket_of(root: &Path, case: &Path) -> Option<String> {
    let rel = case.strip_prefix(root).ok()?;
    rel.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
}

/// Score one case; returns (diff3_correct, llm_edit_correct).
fn score_case(case: &Path) -> Result<(bool, bool)> {
    if is_multi_merge_case(case) {
        let llm = score_llm_multi_merge(case).unwrap_or(false);
        return Ok((false, llm));
    }
    let is_positive = case.join("expected").is_dir();
    let is_negative = case.join("conflict.txt").is_file();

    let llm = score_llm_edit(case, is_positive)?;
    let d3 = if is_positive {
        score_diff3_positive(case)?
    } else if is_negative {
        score_diff3_negative(case)?
    } else {
        false
    };
    Ok((d3, llm))
}

/// Run the 2-generation chained merge with persistence and compare the
/// final tree against `expected/`. Returns `Ok(true)` only on exact match.
fn score_llm_multi_merge(case: &Path) -> Result<bool> {
    let base = load_tree(&case.join("gen0"))?;
    let log_a = parse_log(&fs::read_to_string(case.join("gen1_left.jsonl"))?)?;
    let log_b = match fs::read_to_string(case.join("gen1_right.jsonl")) {
        Ok(text) => parse_log(&text)?,
        Err(_) => Vec::<Op>::new(),
    };
    let log_c = parse_log(&fs::read_to_string(case.join("gen2.jsonl"))?)?;

    let (gen1_tree, gen1_log) = match merge_trees_and_ops(base, &log_a, &log_b)? {
        MergeResult::Merged { tree, log } => (tree, log),
        MergeResult::Conflicts(_) => return Ok(false),
    };
    let gen2_tree = match merge_trees_and_ops(gen1_tree, &gen1_log, &log_c)? {
        MergeResult::Merged { tree, .. } => tree,
        MergeResult::Conflicts(_) => return Ok(false),
    };
    let issues = compare_tree(&gen2_tree, &case.join("expected"))?;
    Ok(issues.is_empty())
}

fn score_llm_edit(case: &Path, is_positive: bool) -> Result<bool> {
    let log_a = case.join("agent_a/.llm-edit/log.jsonl");
    let log_b = case.join("agent_b/.llm-edit/log.jsonl");
    if !log_a.exists() || !log_b.exists() {
        // No structured ops available: llm-edit cannot merge this case.
        return Ok(false);
    }
    let outcome = match merge_case(case) {
        Ok(o) => o,
        Err(_) => return Ok(false),
    };
    if is_positive {
        let expected_dir = case.join("expected");
        match outcome {
            MergeResult::Merged { tree, .. } => {
                let issues = compare_tree(&tree, &expected_dir)?;
                Ok(issues.is_empty())
            }
            MergeResult::Conflicts(_) => Ok(false),
        }
    } else {
        let conflict_file = case.join("conflict.txt");
        match outcome {
            MergeResult::Conflicts(c) => {
                let issues = compare_conflicts(&c, &conflict_file)?;
                Ok(issues.is_empty())
            }
            MergeResult::Merged { .. } => Ok(false),
        }
    }
}

/// Positive: every file merges cleanly (exit 0, no markers) AND matches expected.
fn score_diff3_positive(case: &Path) -> Result<bool> {
    let base = case.join("base");
    let a = case.join("agent_a");
    let b = case.join("agent_b");
    let expected = case.join("expected");

    let rels = enumerate_files(&[&base, &a, &b, &expected])?;
    if rels.is_empty() {
        return Ok(false);
    }
    for rel in rels {
        let bp = base.join(&rel);
        let ap = a.join(&rel);
        let bp2 = b.join(&rel);
        let ep = expected.join(&rel);
        // Any missing side = diff3 can't do it → WRONG.
        if !bp.exists() || !ap.exists() || !bp2.exists() || !ep.exists() {
            return Ok(false);
        }
        let (code, merged) = diff3_merge(&ap, &bp, &bp2)?;
        if code != 0 {
            return Ok(false);
        }
        if merged.contains("<<<<<<<") || merged.contains(">>>>>>>") {
            return Ok(false);
        }
        let want = fs::read_to_string(&ep).with_context(|| format!("read {}", ep.display()))?;
        if merged != want {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Negative: at least one file produces conflict markers.
fn score_diff3_negative(case: &Path) -> Result<bool> {
    let base = case.join("base");
    let a = case.join("agent_a");
    let b = case.join("agent_b");
    let rels = enumerate_files(&[&base, &a, &b])?;
    for rel in rels {
        let bp = base.join(&rel);
        let ap = a.join(&rel);
        let bp2 = b.join(&rel);
        if !bp.exists() || !ap.exists() || !bp2.exists() {
            continue;
        }
        let (_code, merged) = diff3_merge(&ap, &bp, &bp2)?;
        if merged.contains("<<<<<<<") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn diff3_merge(mine: &Path, older: &Path, yours: &Path) -> Result<(i32, String)> {
    let output = Command::new("diff3")
        .arg("-m")
        .arg(mine)
        .arg(older)
        .arg(yours)
        .output()
        .context("spawn diff3")?;
    let code = output.status.code().unwrap_or(-1);
    let merged = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((code, merged))
}

fn enumerate_files(roots: &[&PathBuf]) -> Result<Vec<PathBuf>> {
    let mut set = BTreeSet::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(root)
                .with_context(|| format!("strip_prefix {}", root.display()))?;
            // Skip op-log artefacts.
            if rel.starts_with(".llm-edit") {
                continue;
            }
            set.insert(rel.to_path_buf());
        }
    }
    Ok(set.into_iter().collect())
}

fn print_table(buckets: &BTreeMap<String, Tally>, total: &Tally) -> Result<bool> {
    let name_col_w = buckets
        .keys()
        .map(|k| k.len())
        .max()
        .unwrap_or(16)
        .max("bucket".len())
        .max("TOTAL".len());

    let dash = "-".repeat(name_col_w);
    let header = format!(
        "{:<w$}  {:<8}   {:<8}",
        "bucket",
        "diff3",
        "llm-edit",
        w = name_col_w
    );
    let sep = format!("{}  {}   {}", dash, "-".repeat(8), "-".repeat(8));
    println!("{}", header);
    println!("{}", sep);

    for (name, t) in buckets {
        println!(
            "{:<w$}  {:<8}   {:<8}",
            name,
            format!("{}/{}", t.diff3_correct, t.total),
            format!("{}/{}", t.llm_correct, t.total),
            w = name_col_w
        );
    }
    println!("{}", sep);

    if total.total == 0 {
        return Err(anyhow!("no cases scored"));
    }
    let d_pct = total.diff3_correct as f64 / total.total as f64;
    let l_pct = total.llm_correct as f64 / total.total as f64;
    let gap = l_pct - d_pct;
    let pass = gap >= 0.90;
    let sign = if gap >= 0.0 { "+" } else { "-" };
    let badge = if pass { "\u{2705} PASS" } else { "\u{274C} FAIL" };
    println!(
        "{:<w$}  {:<8}   {:<8}   gap={}{:.2}  {}",
        "TOTAL",
        format!("{}/{}", total.diff3_correct, total.total),
        format!("{}/{}", total.llm_correct, total.total),
        sign,
        gap.abs(),
        badge,
        w = name_col_w
    );
    Ok(pass)
}
