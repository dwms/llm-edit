// Token-savings harness: walk the same fixtures the bench uses; for
// each case where diff3 produces a conflict (or wrong output) but
// llm-edit resolves cleanly, estimate the tokens an LLM resolver would
// have spent (input = conflicted file bytes, output = corrected file
// bytes).
//
// Token model: chars/4 — the standard rough estimate for English/code
// tokenization. Good enough for comparative numbers; not a billing
// substitute.
//
// Usage:
//   cargo run --release --bin token_savings [path-to-fixtures-root]

use anyhow::{Context, Result, anyhow};
use llm_edit::merge_logic::{MergeResult, compare_tree, merge_case};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use walkdir::WalkDir;

#[derive(Default, Debug, Clone, Copy)]
struct Bucket {
    cases_total: usize,
    cases_llm_saves: usize,
    tokens_saved: usize,
    diff3_input_chars: usize,
    expected_chars: usize,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("token_savings: {:#}", e);
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/benchmark"));
    let cases = discover_cases(&root);
    if cases.is_empty() {
        println!("(no fixtures at {})", root.display());
        return Ok(());
    }

    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut total = Bucket::default();

    for case in &cases {
        let bucket_name = bucket_of(&root, case).unwrap_or_else(|| "<unbucketed>".to_string());
        let bucket = buckets.entry(bucket_name).or_default();
        bucket.cases_total += 1;
        total.cases_total += 1;

        // Only positive cases (with expected/) qualify — we need a
        // ground-truth resolution to measure.
        let expected_dir = case.join("expected");
        if !expected_dir.is_dir() {
            continue;
        }

        // Did diff3 fail on this case?
        let (d3_clean, d3_input_chars) = diff3_outcome(case)?;
        // Did llm-edit succeed?
        let llm_ok = llm_edit_outcome(case, &expected_dir)?;

        if !d3_clean && llm_ok {
            // Tokens an agent would have spent resolving:
            //   input  = conflicted file content (with markers)
            //   output = correct merged file content
            let exp_chars = sum_tree_chars(&expected_dir)?;
            let saved = (d3_input_chars + exp_chars) / 4;
            bucket.cases_llm_saves += 1;
            bucket.tokens_saved += saved;
            bucket.diff3_input_chars += d3_input_chars;
            bucket.expected_chars += exp_chars;
            total.cases_llm_saves += 1;
            total.tokens_saved += saved;
            total.diff3_input_chars += d3_input_chars;
            total.expected_chars += exp_chars;
        }
    }

    print_table(&buckets, &total);
    Ok(())
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
        if has_base && has_a && has_b && has_expected {
            out.push(p.to_path_buf());
        }
    }
    out.sort();
    out
}

fn bucket_of(root: &Path, case: &Path) -> Option<String> {
    let rel = case.strip_prefix(root).ok()?;
    rel.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
}

/// Returns (diff3_clean_match_expected, total_chars_resolver_would_see).
/// `diff3_clean_match_expected` is true iff every file merges cleanly
/// AND matches expected — i.e., diff3 fully solved the case.
/// `total_chars_resolver_would_see` is the sum of conflicted-file
/// character counts (or merged output for files diff3 handled cleanly,
/// since a resolver still needs to read those to verify).
fn diff3_outcome(case: &Path) -> Result<(bool, usize)> {
    let base = case.join("base");
    let a = case.join("agent_a");
    let b = case.join("agent_b");
    let expected = case.join("expected");

    let rels = enumerate_files(&[&base, &a, &b, &expected])?;
    if rels.is_empty() {
        return Ok((false, 0));
    }
    let mut all_clean = true;
    let mut conflict_chars = 0usize;
    for rel in rels {
        let bp = base.join(&rel);
        let ap = a.join(&rel);
        let bp2 = b.join(&rel);
        let ep = expected.join(&rel);
        if !bp.exists() || !ap.exists() || !bp2.exists() || !ep.exists() {
            all_clean = false;
            // A missing-side file means the resolver sees both versions.
            let a_chars = file_chars(&ap).unwrap_or(0);
            let b_chars = file_chars(&bp2).unwrap_or(0);
            conflict_chars += a_chars.max(b_chars);
            continue;
        }
        let (code, merged) = diff3_merge(&ap, &bp, &bp2)?;
        let has_markers = merged
            .lines()
            .any(|l| l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>"));
        if code != 0 || has_markers {
            all_clean = false;
            conflict_chars += merged.chars().count();
            continue;
        }
        // Clean merge — but if it doesn't match expected, it's still wrong;
        // a resolver would have to compare and fix.
        let want = fs::read_to_string(&ep).unwrap_or_default();
        if merged != want {
            all_clean = false;
            conflict_chars += merged.chars().count();
        }
    }
    Ok((all_clean, conflict_chars))
}

fn llm_edit_outcome(case: &Path, expected_dir: &Path) -> Result<bool> {
    let log_a = case.join("agent_a/.llm-edit/log.jsonl");
    let log_b = case.join("agent_b/.llm-edit/log.jsonl");
    if !log_a.exists() || !log_b.exists() {
        return Ok(false);
    }
    let outcome = match merge_case(case) {
        Ok(o) => o,
        Err(_) => return Ok(false),
    };
    match outcome {
        MergeResult::Merged { tree, .. } => {
            let issues = compare_tree(&tree, expected_dir)?;
            Ok(issues.is_empty())
        }
        MergeResult::Conflicts(_) => Ok(false),
    }
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
            if rel.starts_with(".llm-edit") {
                continue;
            }
            set.insert(rel.to_path_buf());
        }
    }
    Ok(set.into_iter().collect())
}

fn file_chars(p: &Path) -> Result<usize> {
    Ok(fs::read_to_string(p)
        .map(|s| s.chars().count())
        .unwrap_or(0))
}

fn sum_tree_chars(root: &Path) -> Result<usize> {
    let mut total = 0usize;
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .strip_prefix(root)
            .map(|p| p.starts_with(".llm-edit"))
            .unwrap_or(false)
        {
            continue;
        }
        total += file_chars(entry.path()).unwrap_or(0);
    }
    Ok(total)
}

fn print_table(buckets: &BTreeMap<String, Bucket>, total: &Bucket) {
    let name_w = buckets
        .keys()
        .map(|k| k.len())
        .max()
        .unwrap_or(16)
        .max("bucket".len())
        .max("TOTAL".len());

    println!(
        "{:<w$}  {:>10}  {:>14}  {:>14}",
        "bucket",
        "llm-saves",
        "tokens-saved",
        "avg-per-case",
        w = name_w
    );
    let dash = "-".repeat(name_w);
    println!(
        "{}  {}  {}  {}",
        dash,
        "-".repeat(10),
        "-".repeat(14),
        "-".repeat(14)
    );
    for (name, b) in buckets {
        let saves_str = format!("{}/{}", b.cases_llm_saves, b.cases_total);
        let tokens_str = fmt_int(b.tokens_saved);
        let avg = if b.cases_llm_saves > 0 {
            fmt_int(b.tokens_saved / b.cases_llm_saves)
        } else {
            "-".to_string()
        };
        println!(
            "{:<w$}  {:>10}  {:>14}  {:>14}",
            name,
            saves_str,
            tokens_str,
            avg,
            w = name_w
        );
    }
    println!(
        "{}  {}  {}  {}",
        dash,
        "-".repeat(10),
        "-".repeat(14),
        "-".repeat(14)
    );
    let saves_str = format!("{}/{}", total.cases_llm_saves, total.cases_total);
    let tokens_str = fmt_int(total.tokens_saved);
    let avg = if total.cases_llm_saves > 0 {
        fmt_int(total.tokens_saved / total.cases_llm_saves)
    } else {
        "-".to_string()
    };
    println!(
        "{:<w$}  {:>10}  {:>14}  {:>14}",
        "TOTAL",
        saves_str,
        tokens_str,
        avg,
        w = name_w
    );
    let _ = anyhow!("");
}

fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let bytes: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out
}
