// Merge two agents' op-logs on top of a common base and compare with
// `expected/`. Implements the v0 merge algorithm from ops.md:
//
//   1. Apply A's ops to base.
//   2. Inspect A's ops to build a `Rewrites` record describing the
//      refactors we need to propagate (renames, extractions, …).
//   3. For each of B's ops, rewrite its payload/anchor through A's
//      refactors so that B's intent lands in the post-A tree.
//   4. Apply rewritten B's ops.
//   5. Diff the result against expected/.
//
// Only covers what the 3 logged fixtures exercise. Every unhandled shape
// fails loudly, not silently — fixtures without logs aren't merged.

use anyhow::{Context, Result, anyhow};
use llm_edit::ops::{Op, parse_log};
use llm_edit::{
    Tree, apply_log, load_tree, rename_css_selector_class, rename_html_class_attr,
    rename_word_boundary,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ConflictReport {
    RenameSymbolMismatch {
        file: String,
        old: String,
        a_new: String,
        b_new: String,
    },
    DeleteVsEdit {
        file: String,
        entity: String,
    },
    ReplaceInAnchorMismatch {
        file: String,
        #[serde(rename = "fn")]
        fn_name: String,
        old: String,
        a_new: String,
        b_new: String,
    },
}

enum MergeResult {
    Merged(Tree),
    Conflicts(Vec<ConflictReport>),
}

#[derive(Default, Debug)]
struct Rewrites {
    /// Module-scoped identifier renames: `old → new` in a specific file.
    symbol_renames: Vec<SymbolRename>,
    /// Project-scoped identifier renames: `old → new` across JS + HTML.
    project_symbol_renames: Vec<(String, String)>,
    /// Project-scoped CSS class renames.
    class_renames: Vec<(String, String)>,
    /// Function-body extractions by A, so B's ops that targeted the source
    /// body can follow content into the new helper.
    extractions: Vec<Extraction>,
}

#[derive(Debug)]
struct SymbolRename {
    file: String,
    old: String,
    new: String,
}

#[derive(Debug)]
struct Extraction {
    file: String,
    from_fn: String,
    to_fn: String,
    /// Snapshot of the statements moved out, used to detect whether B's
    /// `replace_in_anchor` targets content that now lives in `to_fn`.
    moved_text: String,
}

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

/// Merge the case and compare the outcome against either `expected/`
/// (positive fixture) or `conflict.txt` (negative fixture). Returns a
/// list of human-readable issues — empty means PASS.
fn check_case(case: &Path) -> Result<Vec<String>> {
    let expected_dir = case.join("expected");
    let conflict_file = case.join("conflict.txt");
    let outcome = merge_case(case)?;

    if expected_dir.is_dir() {
        match outcome {
            MergeResult::Merged(tree) => compare_tree(&tree, &expected_dir),
            MergeResult::Conflicts(conflicts) => Ok(vec![format!(
                "expected merged output, got {} conflict(s): {}",
                conflicts.len(),
                conflicts
                    .iter()
                    .map(|c| serde_json::to_string(c).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("; ")
            )]),
        }
    } else if conflict_file.is_file() {
        match outcome {
            MergeResult::Conflicts(conflicts) => compare_conflicts(&conflicts, &conflict_file),
            MergeResult::Merged(_) => Ok(vec![
                "expected conflicts, got clean merge (merger missed the disagreement)".to_string(),
            ]),
        }
    } else {
        Ok(vec![format!(
            "case has neither expected/ nor conflict.txt: {}",
            case.display()
        )])
    }
}

fn compare_tree(tree: &Tree, expected_dir: &Path) -> Result<Vec<String>> {
    let expected = load_tree(expected_dir)?;
    let mut issues = Vec::new();
    let all: BTreeSet<&PathBuf> = tree.keys().chain(expected.keys()).collect();
    for path in all {
        match (tree.get(path), expected.get(path)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => issues.push(format!("{}: contents differ", path.display())),
            (Some(_), None) => issues.push(format!("{}: only in merged", path.display())),
            (None, Some(_)) => issues.push(format!("{}: only in expected", path.display())),
            (None, None) => unreachable!(),
        }
    }
    Ok(issues)
}

fn compare_conflicts(got: &[ConflictReport], expected_file: &Path) -> Result<Vec<String>> {
    let want_text = std::fs::read_to_string(expected_file)?;
    let want: Vec<ConflictReport> = want_text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|l| serde_json::from_str::<ConflictReport>(l))
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("parse {}", expected_file.display()))?;

    let mut got_sorted = got.to_vec();
    got_sorted.sort();
    let mut want_sorted = want;
    want_sorted.sort();

    if got_sorted == want_sorted {
        Ok(Vec::new())
    } else {
        let got_json: Vec<_> = got_sorted
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        let want_json: Vec<_> = want_sorted
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        Ok(vec![format!(
            "conflict reports differ\n        got:  {}\n        want: {}",
            got_json.join("\n              "),
            want_json.join("\n              ")
        )])
    }
}

fn merge_case(case: &Path) -> Result<MergeResult> {
    let base = case.join("base");
    let mut tree = load_tree(&base)?;
    let log_a = std::fs::read_to_string(case.join("agent_a/.llm-edit/log.jsonl"))?;
    let log_b = std::fs::read_to_string(case.join("agent_b/.llm-edit/log.jsonl"))?;
    let ops_a = parse_log(&log_a)?;
    let ops_b = parse_log(&log_b)?;

    // Semantic conflict detection runs before any tree mutation. If any
    // detector fires, we stop and emit reports instead of a merged tree.
    let conflicts = detect_conflicts(&ops_a, &ops_b);
    if !conflicts.is_empty() {
        return Ok(MergeResult::Conflicts(conflicts));
    }

    // Snapshot base before A applies; we need extracted statement text.
    let base_tree = tree.clone();

    // Precondition check: scan B for variables it reassigns, then drop any
    // of A's retype_binding ops whose precondition (`reassigned: false`) is
    // invalidated by B. (Distinct from detect_conflicts: this is a
    // resolvable semantic override, not a disagreement.)
    let b_reassigns = variables_reassigned_by(&ops_b);
    let ops_a: Vec<Op> = ops_a
        .into_iter()
        .filter(|op| !retype_invalidated_by_b(op, &b_reassigns))
        .collect();

    apply_log(&mut tree, &ops_a).context("apply A")?;
    let rewrites = collect_rewrites(&ops_a, &base_tree)?;
    let ops_b_rewritten: Vec<Op> = ops_b
        .into_iter()
        .map(|op| rewrite_b_op(op, &rewrites))
        .collect::<Result<Vec<_>>>()?;
    apply_log(&mut tree, &ops_b_rewritten).context("apply rewritten B")?;

    Ok(MergeResult::Merged(tree))
}

fn detect_conflicts(ops_a: &[Op], ops_b: &[Op]) -> Vec<ConflictReport> {
    let mut out = Vec::new();

    // (1) rename_symbol mismatch: same (file, old) with different `new`.
    for a in ops_a {
        if let Op::RenameSymbol { scope: a_scope, file: a_file, old: a_old, new: a_new, .. } = a {
            if a_scope != "module" {
                continue;
            }
            for b in ops_b {
                if let Op::RenameSymbol { scope: b_scope, file: b_file, old: b_old, new: b_new, .. } = b {
                    if b_scope == "module" && a_file == b_file && a_old == b_old && a_new != b_new {
                        out.push(ConflictReport::RenameSymbolMismatch {
                            file: a_file.clone(),
                            old: a_old.clone(),
                            a_new: a_new.clone(),
                            b_new: b_new.clone(),
                        });
                    }
                }
            }
        }
    }

    // (2) delete_function vs any op on the same function.
    for a in ops_a {
        if let Op::DeleteFunction { anchor } = a {
            let file = anchor.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let fn_name = extract_fn_name_from_path(anchor).ok();
            if let Some(fn_name) = fn_name {
                for b in ops_b {
                    if op_touches_function(b, &file, &fn_name) {
                        out.push(ConflictReport::DeleteVsEdit {
                            file: file.clone(),
                            entity: format!("function:{}", fn_name),
                        });
                    }
                }
            }
        }
    }
    // symmetric: delete_function in B, edit in A.
    for b in ops_b {
        if let Op::DeleteFunction { anchor } = b {
            let file = anchor.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let fn_name = extract_fn_name_from_path(anchor).ok();
            if let Some(fn_name) = fn_name {
                for a in ops_a {
                    if op_touches_function(a, &file, &fn_name) {
                        out.push(ConflictReport::DeleteVsEdit {
                            file: file.clone(),
                            entity: format!("function:{}", fn_name),
                        });
                    }
                }
            }
        }
    }

    // (3) replace_in_anchor with same (file, fn, old) but different `new`.
    for a in ops_a {
        if let Op::ReplaceInAnchor { anchor: a_anc, old: a_old, new: a_new } = a {
            let a_file = a_anc.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let a_fn = match extract_fn_name_from_path(a_anc) {
                Ok(n) => n,
                Err(_) => continue, // only supported shape here
            };
            for b in ops_b {
                if let Op::ReplaceInAnchor { anchor: b_anc, old: b_old, new: b_new } = b {
                    let b_file = b_anc.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let b_fn = match extract_fn_name_from_path(b_anc) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    if a_file == b_file && a_fn == b_fn && a_old == b_old && a_new != b_new {
                        out.push(ConflictReport::ReplaceInAnchorMismatch {
                            file: a_file.clone(),
                            fn_name: a_fn.clone(),
                            old: a_old.clone(),
                            a_new: a_new.clone(),
                            b_new: b_new.clone(),
                        });
                    }
                }
            }
        }
    }

    // Deduplicate while preserving order (same conflict can fire from
    // symmetric passes).
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(serde_json::to_string(c).unwrap()));
    out
}

/// Return true if `op`'s anchor targets a given function in `file`.
fn op_touches_function(op: &Op, file: &str, fn_name: &str) -> bool {
    let anchor_fn = |anc: &serde_json::Value| -> Option<(String, String)> {
        let f = anc.get("file").and_then(|v| v.as_str())?.to_string();
        let n = extract_fn_name_from_path(anc).ok()?;
        Some((f, n))
    };
    match op {
        Op::ReplaceInAnchor { anchor, .. }
        | Op::DeleteNode { anchor }
        | Op::ExtractFunction { source: anchor, .. } => {
            if let Some((f, n)) = anchor_fn(anchor) {
                return f == file && n == fn_name;
            }
            false
        }
        _ => false,
    }
}

fn variables_reassigned_by(ops: &[Op]) -> HashSet<String> {
    let mut set = HashSet::new();
    for op in ops {
        if let Op::InsertAfter { reassigns, .. } = op {
            for v in reassigns {
                set.insert(v.clone());
            }
        }
    }
    set
}

fn retype_invalidated_by_b(op: &Op, b_reassigns: &HashSet<String>) -> bool {
    if let Op::RetypeBinding { anchor, precondition, .. } = op {
        let wants_unreassigned = precondition
            .get("reassigned")
            .and_then(|v| v.as_bool())
            == Some(false);
        if !wants_unreassigned {
            return false;
        }
        let var = anchor
            .get("variable")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return b_reassigns.contains(var);
    }
    false
}

fn collect_rewrites(
    ops: &[Op],
    base_tree: &llm_edit::Tree,
) -> Result<Rewrites> {
    let mut r = Rewrites::default();
    for op in ops {
        match op {
            Op::RenameSymbol { scope, file, old, new, .. } if scope == "module" => {
                r.symbol_renames.push(SymbolRename {
                    file: file.clone(),
                    old: old.clone(),
                    new: new.clone(),
                });
            }
            Op::RenameSymbol { scope, old, new, .. } if scope == "project" => {
                r.project_symbol_renames.push((old.clone(), new.clone()));
            }
            Op::RenameCssClass { scope, old, new } if scope == "project" => {
                r.class_renames.push((old.clone(), new.clone()));
            }
            Op::ExtractFunction {
                source, body_range, new_name, ..
            } => {
                // Snapshot the statements moved out of the source function
                // so we can match them against B's ops later.
                let file = source
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("extract source missing file"))?
                    .to_string();
                let from_fn = extract_fn_name_from_path(source)?;
                let base_text = base_tree
                    .get(&PathBuf::from(&file))
                    .ok_or_else(|| anyhow!("base missing {}", file))?;
                let moved_text = snapshot_fn_body_stmts(base_text, &from_fn, *body_range)?;
                r.extractions.push(Extraction {
                    file,
                    from_fn,
                    to_fn: new_name.clone(),
                    moved_text,
                });
            }
            _ => {}
        }
    }
    Ok(r)
}

fn extract_fn_name_from_path(anchor: &serde_json::Value) -> Result<String> {
    let path = anchor
        .get("path")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("anchor.path missing"))?;
    for seg in path {
        if let Some(obj) = seg.as_object() {
            if obj.get("kind").and_then(|v| v.as_str()) == Some("function_def") {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    return Ok(name.to_string());
                }
            }
        }
    }
    Err(anyhow!("no function_def segment in anchor"))
}

/// Return the concatenated text of the statements in `fn_name`'s body
/// whose indices fall within `[lo, hi]` inclusive (indices over top-level
/// body statements, same definition as the applier's extract_function).
fn snapshot_fn_body_stmts(
    text: &str,
    fn_name: &str,
    range: [usize; 2],
) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent = lines[def_idx].len() - lines[def_idx].trim_start().len();
    let body_indent = fn_indent + 4;

    let mut stmts: Vec<(usize, usize)> = Vec::new();
    let mut i = def_idx + 1;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent = line.len() - line.trim_start().len();
        if !is_blank && indent < body_indent {
            break;
        }
        if !is_blank && indent == body_indent {
            stmts.push((i, i));
        } else if let Some(last) = stmts.last_mut() {
            last.1 = i;
        }
        i += 1;
    }
    let [lo, hi] = range;
    if hi >= stmts.len() {
        return Err(anyhow!("range out of bounds"));
    }
    let out: String = (stmts[lo].0..=stmts[hi].1)
        .map(|j| lines[j])
        .collect::<Vec<_>>()
        .concat();
    Ok(out)
}

fn rewrite_b_op(op: Op, r: &Rewrites) -> Result<Op> {
    Ok(match op {
        Op::AddFunction { anchor, after, name, source } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut source = source;
            for rn in &r.symbol_renames {
                if rn.file == file {
                    source = rename_word_boundary(&source, &rn.old, &rn.new);
                }
            }
            Op::AddFunction { anchor, after, name, source }
        }
        Op::InsertElement { anchor, position, html } => {
            let mut html = html;
            for (old, new) in &r.class_renames {
                html = rename_html_class_attr(&html, old, new);
            }
            // Project-scoped symbol renames propagate to any identifier
            // reference inside the inserted HTML (e.g. onclick handler values).
            for (old, new) in &r.project_symbol_renames {
                html = rename_word_boundary(&html, old, new);
            }
            Op::InsertElement { anchor, position, html }
        }
        Op::ReplaceInAnchor { anchor, old, new } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // If A extracted statements out of the function B targets,
            // and the extracted text contains B's `old`, re-point the
            // anchor at the new helper.
            let mut anchor = anchor;
            let current_fn = extract_fn_name_from_path(&anchor).ok();
            if let Some(cur) = current_fn {
                for ext in &r.extractions {
                    if ext.file == file && ext.from_fn == cur && ext.moved_text.contains(&old) {
                        anchor = retarget_fn_name(&anchor, &ext.to_fn)?;
                        break;
                    }
                }
            }
            // Also rewrite `old`/`new` through symbol renames, in case a
            // rename ran first and the substring references the renamed
            // identifier. Not used by current fixtures but cheap & safe.
            let mut old = old;
            let mut new = new;
            for rn in &r.symbol_renames {
                if rn.file == file {
                    old = rename_word_boundary(&old, &rn.old, &rn.new);
                    new = rename_word_boundary(&new, &rn.old, &rn.new);
                }
            }
            Op::ReplaceInAnchor { anchor, old, new }
        }
        Op::AddRule { anchor, selector, declarations } => {
            let mut selector = selector;
            for (old, new) in &r.class_renames {
                selector = rename_css_selector_class(&selector, old, new);
            }
            Op::AddRule { anchor, selector, declarations }
        }
        other => other,
    })
}

/// Replace the `name` field of the first `{kind: "function_def", name: ...}`
/// segment in `anchor.path` with `new_name`.
fn retarget_fn_name(anchor: &serde_json::Value, new_name: &str) -> Result<serde_json::Value> {
    let mut v = anchor.clone();
    let path = v
        .get_mut("path")
        .and_then(|p| p.as_array_mut())
        .ok_or_else(|| anyhow!("anchor.path missing"))?;
    for seg in path.iter_mut() {
        if let Some(obj) = seg.as_object_mut() {
            if obj.get("kind").and_then(|x| x.as_str()) == Some("function_def") {
                obj.insert("name".to_string(), serde_json::Value::String(new_name.to_string()));
                return Ok(v);
            }
        }
    }
    Err(anyhow!("no function_def segment to retarget"))
}
