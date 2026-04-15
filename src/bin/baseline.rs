// Baseline harness: for every fixture case under `fixtures/`, run
// `diff3 -m agent_a base agent_b` on each file and compare with `expected/`.
// Report whether the case is (a) merged silently to a wrong answer,
// (b) produced conflict markers, or (c) matched expected.
//
// This establishes the bar the structured-op merger must beat.

use anyhow::{Context, Result, anyhow};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

const REQUIRED_SUBDIRS: [&str; 4] = ["base", "agent_a", "agent_b", "expected"];

#[derive(Debug)]
enum FileOutcome {
    Match,
    SilentDiff,       // diff3 merged without conflict markers, but result ≠ expected
    ConflictMarkers,  // diff3 emitted <<<<<<< markers
    MergeFailed(i32), // diff3 exit code > 1 (real error)
    Missing(String),  // file missing on one side; diff3 doesn't handle
}

#[derive(Debug)]
struct CaseResult {
    case: PathBuf,
    files: Vec<(PathBuf, FileOutcome)>,
}

fn main() -> Result<()> {
    let root = PathBuf::from("fixtures");
    if !root.exists() {
        return Err(anyhow!("fixtures/ directory not found; run from project root"));
    }

    let cases = discover_cases(&root)?;
    if cases.is_empty() {
        return Err(anyhow!("no fixture cases found under {}", root.display()));
    }

    let mut overall_hard = 0usize;
    let mut overall_easy = 0usize;

    for case in &cases {
        let result = run_case(case)?;
        print_case(&result);
        let hard_for_diff3 = result
            .files
            .iter()
            .any(|(_, o)| !matches!(o, FileOutcome::Match));
        if hard_for_diff3 {
            overall_hard += 1;
        } else {
            overall_easy += 1;
        }
    }

    println!();
    println!(
        "summary: {} case(s) hard for diff3 (good — the bar to beat), {} case(s) already trivial",
        overall_hard, overall_easy
    );
    Ok(())
}

fn discover_cases(root: &Path) -> Result<Vec<PathBuf>> {
    let mut cases = Vec::new();
    for entry in WalkDir::new(root).min_depth(1).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if REQUIRED_SUBDIRS
            .iter()
            .all(|sub| entry.path().join(sub).is_dir())
        {
            cases.push(entry.path().to_path_buf());
        }
    }
    cases.sort();
    Ok(cases)
}

fn run_case(case: &Path) -> Result<CaseResult> {
    let base = case.join("base");
    let a = case.join("agent_a");
    let b = case.join("agent_b");
    let expected = case.join("expected");

    let rels = enumerate_files(&[&base, &a, &b, &expected])?;

    let mut files = Vec::new();
    for rel in rels {
        let outcome = merge_one(&base, &a, &b, &expected, &rel)?;
        files.push((rel, outcome));
    }

    Ok(CaseResult {
        case: case.to_path_buf(),
        files,
    })
}

fn enumerate_files(roots: &[&PathBuf]) -> Result<Vec<PathBuf>> {
    let mut set = BTreeSet::new();
    for root in roots {
        for entry in WalkDir::new(root) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(root)
                    .with_context(|| format!("strip_prefix {}", root.display()))?;
                set.insert(rel.to_path_buf());
            }
        }
    }
    Ok(set.into_iter().collect())
}

fn merge_one(
    base: &Path,
    a: &Path,
    b: &Path,
    expected: &Path,
    rel: &Path,
) -> Result<FileOutcome> {
    let bp = base.join(rel);
    let ap = a.join(rel);
    let bp2 = b.join(rel);
    let ep = expected.join(rel);

    // If file is missing on any side, diff3 can't handle it directly.
    // Report as Missing so we notice — we'll need per-side delete/add ops
    // when we build the structured merger.
    for (name, p) in [("base", &bp), ("agent_a", &ap), ("agent_b", &bp2), ("expected", &ep)] {
        if !p.exists() {
            return Ok(FileOutcome::Missing(name.to_string()));
        }
    }

    // diff3 -m <mine> <older> <yours>   (mine = agent_a, older = base, yours = agent_b)
    let output = Command::new("diff3")
        .arg("-m")
        .arg(&ap)
        .arg(&bp)
        .arg(&bp2)
        .output()
        .context("spawn diff3")?;

    let code = output.status.code().unwrap_or(-1);
    let merged = String::from_utf8_lossy(&output.stdout).into_owned();

    // diff3 -m exit codes: 0 = clean merge, 1 = conflicts present, >1 = error.
    if code > 1 {
        return Ok(FileOutcome::MergeFailed(code));
    }

    if merged.contains("<<<<<<<") || merged.contains(">>>>>>>") {
        return Ok(FileOutcome::ConflictMarkers);
    }

    let want = fs::read_to_string(&ep).with_context(|| format!("read {}", ep.display()))?;
    if merged == want {
        Ok(FileOutcome::Match)
    } else {
        Ok(FileOutcome::SilentDiff)
    }
}

fn print_case(r: &CaseResult) {
    println!("== {} ==", r.case.display());
    for (rel, outcome) in &r.files {
        let tag = match outcome {
            FileOutcome::Match => "ok",
            FileOutcome::SilentDiff => "SILENT-WRONG",
            FileOutcome::ConflictMarkers => "CONFLICT",
            FileOutcome::MergeFailed(c) => {
                println!("  {:<16} {}  (exit {})", "MERGE-FAILED", rel.display(), c);
                continue;
            }
            FileOutcome::Missing(side) => {
                println!("  {:<16} {}  (missing on {})", "MISSING", rel.display(), side);
                continue;
            }
        };
        println!("  {:<16} {}", tag, rel.display());
    }
}
