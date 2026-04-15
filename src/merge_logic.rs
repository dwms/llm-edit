// Merge-logic internals shared between the `merge` binary and the
// `bench` harness. Extracted verbatim from src/bin/merge.rs so both
// binaries agree on what counts as a successful merge or a conflict.

use crate::ops::{Op, parse_log};
use crate::{
    Tree, apply_log, load_tree, rename_css_selector_class, rename_html_class_attr,
    rename_word_boundary,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictReport {
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
    RenameCssClassMismatch {
        old: String,
        a_new: String,
        b_new: String,
    },
    WrapElementMismatch {
        file: String,
        selector: String,
        a_wrapper: String,
        b_wrapper: String,
    },
    RetypePreconditionViolated {
        file: String,
        variable: String,
        from: String,
        to: String,
        invalidated_by: String,
    },
    DeleteVsNewCaller {
        file: String,
        deleted: String,
        caller: String,
    },
    InsertAnchorDeleted {
        file: String,
        #[serde(rename = "fn")]
        fn_name: String,
        b_anchor_line: String,
        deleted_by: String,
    },
    RenameVsDelete {
        file: String,
        symbol: String,
    },
}

pub enum MergeResult {
    /// Successful merge: both the resulting tree AND the merged op log
    /// (A's surviving ops followed by B's rewritten, non-idempotent ops).
    /// The log is what enables persistence across multi-generation merges.
    Merged { tree: Tree, log: Vec<Op> },
    Conflicts(Vec<ConflictReport>),
}

impl MergeResult {
    pub fn tree(&self) -> Option<&Tree> {
        match self {
            MergeResult::Merged { tree, .. } => Some(tree),
            _ => None,
        }
    }
}

#[derive(Default, Debug)]
struct Rewrites {
    symbol_renames: Vec<SymbolRename>,
    project_symbol_renames: Vec<(String, String)>,
    class_renames: Vec<(String, String)>,
    extractions: Vec<Extraction>,
    wraps: Vec<WrapEntry>,
    /// Per-file `(from_keyword, to_keyword, variable)` tuples from A's
    /// retype_binding ops. Used to rewrite B's `insert_after` needles
    /// that reference the pre-retype declaration line.
    retypes: Vec<RetypeEntry>,
}

#[derive(Debug)]
struct RetypeEntry {
    file: String,
    from: String,
    to: String,
    variable: String,
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
    moved_text: String,
}

/// Records that A ran `wrap_element` against `file`/`selector`. We keep
/// the base-tree text of the element's children verbatim so
/// `rewrite_b_op` can tell whether B's needle points at a line A just
/// moved into the new wrapper (and therefore re-indented by +2 spaces).
#[derive(Debug)]
struct WrapEntry {
    file: String,
    base_children: String,
}

/// Full merge pipeline from a case directory: loads base, agent_a/b op-logs,
/// applies conflict detection, rewrites B through A's refactors, applies
/// both, and returns either the merged tree or a set of conflict reports.
pub fn merge_case(case: &Path) -> Result<MergeResult> {
    let base = case.join("base");
    let tree = load_tree(&base)?;
    let log_a = std::fs::read_to_string(case.join("agent_a/.llm-edit/log.jsonl"))?;
    let log_b = std::fs::read_to_string(case.join("agent_b/.llm-edit/log.jsonl"))?;
    let ops_a = parse_log(&log_a)?;
    let ops_b = parse_log(&log_b)?;
    merge_trees_and_ops(tree, &ops_a, &ops_b)
}

/// Core merge: takes a base tree and two agents' op-logs, returns a
/// `MergeResult`. Called by `merge_case` and by multi-merge harnesses
/// that chain merges without round-tripping through disk.
pub fn merge_trees_and_ops(
    tree: Tree,
    ops_a: &[Op],
    ops_b: &[Op],
) -> Result<MergeResult> {
    let mut tree = tree;
    let ops_a_in = ops_a.to_vec();
    let ops_b_in = ops_b.to_vec();
    let ops_a = ops_a_in;
    let ops_b = ops_b_in;

    let conflicts = detect_conflicts(&ops_a, &ops_b);
    if !conflicts.is_empty() {
        return Ok(MergeResult::Conflicts(conflicts));
    }

    let base_tree = tree.clone();

    let b_reassigns = variables_reassigned_by(&ops_b);
    let ops_a: Vec<Op> = ops_a
        .into_iter()
        .filter(|op| !retype_invalidated_by_b(op, &b_reassigns))
        .collect();

    // Enrich: every `extract_function` op in A that doesn't already
    // carry a `moved_text` snapshot gets one captured from the current
    // tree before the op applies. The snapshot is serialized into the
    // merged output log so later-generation merges can reconstruct the
    // extraction without the original base tree.
    let ops_a: Vec<Op> = ops_a
        .into_iter()
        .map(|op| enrich_extract_with_moved_text(op, &tree))
        .collect::<Result<Vec<_>>>()?;

    // Apply A one op at a time, skipping any that is already a no-op
    // against the current tree. This matters for multi-generation merges:
    // `ops_a` may be the merged log from an earlier generation whose
    // refactors are already present in the tree. We still want those ops
    // in the `rewrites` record so that B's payloads get rewritten through
    // them, but we must not re-apply them to the tree.
    for op in &ops_a {
        if op_is_noop_against(op, &tree)? {
            continue;
        }
        apply_log(&mut tree, std::slice::from_ref(op)).context("apply A")?;
    }
    let rewrites = collect_rewrites(&ops_a, &base_tree)?;
    let ops_b_rewritten: Vec<Op> = ops_b
        .into_iter()
        .map(|op| rewrite_b_op(op, &rewrites))
        .collect::<Result<Vec<_>>>()?;

    // Apply B's ops one at a time, skipping any op that is already a no-op
    // against the current tree state. This handles convergent-edit cases
    // where both agents issued the same logical change — A applied it,
    // B's attempt would be a duplicate (add_import already there) or an
    // error (delete_function: target gone, replace_in_anchor: old not
    // found). The op is a "state-based idempotence skip", not a conflict.
    let mut applied_b: Vec<Op> = Vec::new();
    for op in &ops_b_rewritten {
        if op_is_noop_against(op, &tree)? {
            continue;
        }
        apply_log(&mut tree, std::slice::from_ref(op)).context("apply rewritten B")?;
        applied_b.push(op.clone());
    }

    let mut log = ops_a.clone();
    log.extend(applied_b);

    Ok(MergeResult::Merged { tree, log })
}

/// Compare the merged tree against `expected_dir`. Empty vec = match.
pub fn compare_tree(tree: &Tree, expected_dir: &Path) -> Result<Vec<String>> {
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

/// Compare emitted conflict reports against the newline-delimited JSON in
/// `expected_file`. Empty vec = match (order-independent).
pub fn compare_conflicts(got: &[ConflictReport], expected_file: &Path) -> Result<Vec<String>> {
    let want_text = std::fs::read_to_string(expected_file)?;
    let want: Vec<ConflictReport> = want_text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(serde_json::from_str::<ConflictReport>)
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

/// Check an entire case against its oracle (`expected/` or `conflict.txt`).
/// Empty vec = PASS.
pub fn check_case(case: &Path) -> Result<Vec<String>> {
    let expected_dir = case.join("expected");
    let conflict_file = case.join("conflict.txt");
    let outcome = merge_case(case)?;

    if expected_dir.is_dir() {
        match outcome {
            MergeResult::Merged { tree, .. } => compare_tree(&tree, &expected_dir),
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
            MergeResult::Merged { .. } => Ok(vec![
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

/// Return true when `op` would be a no-op applied against `tree` — i.e.
/// the change it intends is already present. This catches the
/// convergent-edit case where A and B issued the same logical op: after
/// A applies, B's op has nothing left to do and would otherwise error.
///
/// Conservative by design: only checks op shapes where "already applied"
/// is unambiguous. Returns false for everything else so ops still run.
fn op_is_noop_against(op: &Op, tree: &Tree) -> Result<bool> {
    match op {
        Op::AddImport { anchor, module } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("add_import.anchor.file missing"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let needle = format!("import {}", module);
            // line-exact (ignoring leading whitespace / trailing newline)
            Ok(text.lines().any(|l| l.trim() == needle))
        }
        Op::AddRule { anchor, selector, .. } => {
            // Already applied iff the stylesheet already contains a
            // selector block starting with `<selector> {`.
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("add_rule.anchor.file missing"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let needle = format!("{} {{", selector);
            Ok(text.contains(&needle))
        }
        Op::AddFunction { anchor, name, .. } => {
            // Already applied iff the target file already has
            // `def <name>(` (Python) or `function <name>(` (JS) or
            // shape-agnostic `<name>(` after a keyword. For v0 the only
            // add_function callers are Python fixtures, so `def` is
            // sufficient; extend if a non-Python case surfaces.
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("add_function.anchor.file missing"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let needle = format!("def {}(", name);
            Ok(text.contains(&needle))
        }
        Op::DeleteFunction { anchor } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("delete_function.anchor.file missing"))?;
            let fn_name = extract_fn_name_from_path(anchor)?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let needle = format!("def {}(", fn_name);
            Ok(!text.contains(&needle))
        }
        Op::ReplaceInAnchor { anchor, old, new } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("replace_in_anchor.anchor.file missing"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            // Already applied iff `old` absent from file AND `new` present.
            // (Both checks are needed: absence alone could mean the
            // anchor's content is simply not there at all.)
            Ok(!text.contains(old.as_str()) && text.contains(new.as_str()))
        }
        Op::RenameSymbol { scope, file, old, .. } => {
            // Already applied iff the old name is no longer visible in the
            // scope the op would rewrite. The underlying applier is itself
            // idempotent (no-match → no-op), but declaring the skip here
            // keeps behavior uniform and lets us document it.
            match scope.as_str() {
                "module" => {
                    let Some(text) = tree.get(&PathBuf::from(file)) else {
                        return Ok(false);
                    };
                    Ok(!contains_word(text, old))
                }
                "project" => Ok(tree.values().all(|t| !contains_word(t, old))),
                _ => Ok(false),
            }
        }
        Op::RenameCssClass { old, .. } => {
            // Idempotent iff no CSS / HTML file still mentions the old class.
            Ok(tree.iter().all(|(path, text)| {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "css" | "html" | "htm" => !text.contains(old.as_str()),
                    _ => true,
                }
            }))
        }
        Op::RetypeBinding { anchor, from, to, .. } => {
            // Already applied iff the file no longer has the `from`
            // declaration for this variable and already has `to`.
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("retype_binding.anchor.file missing"))?;
            let variable = anchor
                .get("variable")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("retype_binding.anchor.variable missing"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let from_needle = format!("{} {}", from, variable);
            let to_needle = format!("{} {}", to, variable);
            Ok(!text.contains(&from_needle) && text.contains(&to_needle))
        }
        Op::ExtractFunction { source, new_name, .. } => {
            // Already applied iff the destination helper already exists
            // in the file — which is how the extract applier leaves things.
            let file = source
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("extract source missing file"))?;
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            let needle = format!("def {}(", new_name);
            Ok(text.contains(&needle))
        }
        Op::SortBlock { anchor, key } => {
            if key != "alphabetic" {
                return Ok(false);
            }
            let Some(file) = anchor.get("file").and_then(|v| v.as_str()) else {
                return Ok(false);
            };
            let Some(text) = tree.get(&PathBuf::from(file)) else {
                return Ok(false);
            };
            Ok(import_block_is_sorted(text))
        }
        _ => Ok(false),
    }
}

/// Word-boundary substring match, consistent with `rename_word_boundary`:
/// `word` matches only when adjacent characters are non-wordy (or at file
/// boundaries). Used by `op_is_noop_against` for symbol-rename dedup.
fn contains_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let wb = word.as_bytes();
    let is_wordy = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i + wb.len() <= bytes.len() {
        if &bytes[i..i + wb.len()] == wb {
            let before_ok = i == 0 || !is_wordy(bytes[i - 1]);
            let after_ok = i + wb.len() == bytes.len() || !is_wordy(bytes[i + wb.len()]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Word-boundary call-site match: matches `word` as a whole identifier that is
/// immediately followed by `(`. Used by the `DeleteVsNewCaller` detector so
/// `legacy(` is not falsely matched by `legacy_handler(`.
fn contains_word_call(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let wb = word.as_bytes();
    let is_wordy = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i + wb.len() < bytes.len() {
        if &bytes[i..i + wb.len()] == wb {
            let before_ok = i == 0 || !is_wordy(bytes[i - 1]);
            let after_ok = bytes[i + wb.len()] == b'(';
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}


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

    for a in ops_a {
        if let Op::ReplaceInAnchor { anchor: a_anc, old: a_old, new: a_new } = a {
            let a_file = a_anc.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let a_fn = match extract_fn_name_from_path(a_anc) {
                Ok(n) => n,
                Err(_) => continue,
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

    // rename_css_class disagreement: same old, different new.
    for a in ops_a {
        if let Op::RenameCssClass { scope: a_scope, old: a_old, new: a_new } = a {
            if a_scope != "project" {
                continue;
            }
            for b in ops_b {
                if let Op::RenameCssClass { scope: b_scope, old: b_old, new: b_new } = b {
                    if b_scope == "project" && a_old == b_old && a_new != b_new {
                        out.push(ConflictReport::RenameCssClassMismatch {
                            old: a_old.clone(),
                            a_new: a_new.clone(),
                            b_new: b_new.clone(),
                        });
                    }
                }
            }
        }
    }

    // wrap_element disagreement: same file+selector, different wrapper.
    for a in ops_a {
        if let Op::WrapElement { anchor: a_anc, wrapper: a_w } = a {
            let a_file = a_anc.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let a_sel = a_anc.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            for b in ops_b {
                if let Op::WrapElement { anchor: b_anc, wrapper: b_w } = b {
                    let b_file = b_anc.get("file").and_then(|v| v.as_str()).unwrap_or("");
                    let b_sel = b_anc.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                    if a_file == b_file && a_sel == b_sel && a_w != b_w {
                        out.push(ConflictReport::WrapElementMismatch {
                            file: a_file.to_string(),
                            selector: a_sel.to_string(),
                            a_wrapper: a_w.clone(),
                            b_wrapper: b_w.clone(),
                        });
                    }
                }
            }
        }
    }

    // retype_binding precondition violated by B's reassignment — emit
    // conflict unless the op explicitly asked to drop on violation.
    let b_reassigns = variables_reassigned_by(ops_b);
    for a in ops_a {
        if let Op::RetypeBinding { anchor, from, to, precondition, on_violation } = a {
            let wants_unreassigned =
                precondition.get("reassigned").and_then(|v| v.as_bool()) == Some(false);
            if !wants_unreassigned || on_violation == "drop" {
                continue;
            }
            let var = anchor
                .get("variable")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if b_reassigns.contains(&var) {
                let file = anchor
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                out.push(ConflictReport::RetypePreconditionViolated {
                    file,
                    variable: var,
                    from: from.clone(),
                    to: to.clone(),
                    invalidated_by: "new_reassignment".to_string(),
                });
            }
        }
    }
    // Same check, symmetric: B's retype invalidated by A's reassignment.
    let a_reassigns = variables_reassigned_by(ops_a);
    for b in ops_b {
        if let Op::RetypeBinding { anchor, from, to, precondition, on_violation } = b {
            let wants_unreassigned =
                precondition.get("reassigned").and_then(|v| v.as_bool()) == Some(false);
            if !wants_unreassigned || on_violation == "drop" {
                continue;
            }
            let var = anchor
                .get("variable")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if a_reassigns.contains(&var) {
                let file = anchor
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                out.push(ConflictReport::RetypePreconditionViolated {
                    file,
                    variable: var,
                    from: from.clone(),
                    to: to.clone(),
                    invalidated_by: "new_reassignment".to_string(),
                });
            }
        }
    }

    // delete_function vs new caller that references the deleted name.
    let check_delete_vs_caller =
        |del_ops: &[Op], add_ops: &[Op], out: &mut Vec<ConflictReport>| {
            for a in del_ops {
                if let Op::DeleteFunction { anchor } = a {
                    let file = anchor
                        .get("file")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let deleted = match extract_fn_name_from_path(anchor) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    for b in add_ops {
                        if let Op::AddFunction {
                            anchor: _b_anc,
                            name,
                            source,
                            ..
                        } = b
                        {
                            // Cross-file: a new caller in any file referencing
                            // the deleted name still constitutes a conflict.
                            // Word-boundary match on the name, then confirm a
                            // `(` follows to avoid false positives like
                            // `legacy_handler(` matching a deleted `legacy`.
                            if contains_word_call(source, &deleted) {
                                    file: file.clone(),
                                    deleted: deleted.clone(),
                                    caller: name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        };
    check_delete_vs_caller(ops_a, ops_b, &mut out);
    check_delete_vs_caller(ops_b, ops_a, &mut out);

    // A's replace_in_anchor deletes a line B's insert_after needs.
    let check_insert_anchor_deleted =
        |replace_ops: &[Op], insert_ops: &[Op], out: &mut Vec<ConflictReport>| {
            for a in replace_ops {
                if let Op::ReplaceInAnchor { anchor, old, new } = a {
                    let file = anchor
                        .get("file")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let fn_name = match extract_fn_name_from_path(anchor) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    for b in insert_ops {
                        if let Op::InsertAfter { anchor: b_anc, .. } = b {
                            let b_file = b_anc
                                .get("file")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let needle = b_anc
                                .get("after_line_contains")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if b_file == file && old.contains(needle) && !new.contains(needle) {
                                out.push(ConflictReport::InsertAnchorDeleted {
                                    file: file.clone(),
                                    fn_name: fn_name.clone(),
                                    b_anchor_line: needle.to_string(),
                                    deleted_by: "replace_in_anchor".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        };
    check_insert_anchor_deleted(ops_a, ops_b, &mut out);
    check_insert_anchor_deleted(ops_b, ops_a, &mut out);

    // rename_symbol vs delete_function targeting the same function.
    let check_rename_vs_delete =
        |rename_ops: &[Op], delete_ops: &[Op], out: &mut Vec<ConflictReport>| {
            for a in rename_ops {
                if let Op::RenameSymbol {
                    scope,
                    file: r_file,
                    old,
                    ..
                } = a
                {
                    if scope != "module" {
                        continue;
                    }
                    for b in delete_ops {
                        if let Op::DeleteFunction { anchor } = b {
                            let b_file = anchor
                                .get("file")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let b_name = extract_fn_name_from_path(anchor).ok();
                            if b_file == *r_file && b_name.as_deref() == Some(old.as_str()) {
                                out.push(ConflictReport::RenameVsDelete {
                                    file: b_file,
                                    symbol: old.clone(),
                                });
                            }
                        }
                    }
                }
            }
        };
    check_rename_vs_delete(ops_a, ops_b, &mut out);
    check_rename_vs_delete(ops_b, ops_a, &mut out);

    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(serde_json::to_string(c).unwrap()));
    out
}

/// If `op` is `extract_function` without a `moved_text` snapshot, capture
/// one from the current tree's source function body so the op — once
/// written to the merged log — is self-describing for later-generation
/// merges. Non-extract ops pass through unchanged.
fn enrich_extract_with_moved_text(op: Op, tree: &Tree) -> Result<Op> {
    if let Op::ExtractFunction {
        source,
        body_range,
        new_name,
        params,
        returns,
        moved_text,
    } = op
    {
        if moved_text.is_some() {
            return Ok(Op::ExtractFunction {
                source,
                body_range,
                new_name,
                params,
                returns,
                moved_text,
            });
        }
        let file = source
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("extract source missing file"))?;
        let from_fn = extract_fn_name_from_path(&source)?;
        let captured = match tree.get(&PathBuf::from(file)) {
            Some(text) => snapshot_fn_body_stmts(text, &from_fn, body_range).ok(),
            None => None,
        };
        return Ok(Op::ExtractFunction {
            source,
            body_range,
            new_name,
            params,
            returns,
            moved_text: captured,
        });
    }
    Ok(op)
}

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

/// The precondition-filter version: drop A's retype only when policy is
/// `drop`. The `conflict` policy is handled by `detect_conflicts` above,
/// which emits a `RetypePreconditionViolated` report instead.
fn retype_invalidated_by_b(op: &Op, b_reassigns: &HashSet<String>) -> bool {
    if let Op::RetypeBinding { anchor, precondition, on_violation, .. } = op {
        if on_violation != "drop" {
            return false; // conflict path is handled by detect_conflicts
        }
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

fn collect_rewrites(ops: &[Op], base_tree: &Tree) -> Result<Rewrites> {
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
            Op::RetypeBinding { anchor, from, to, .. } => {
                let file = anchor
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let variable = anchor
                    .get("variable")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !file.is_empty() && !variable.is_empty() {
                    r.retypes.push(RetypeEntry {
                        file,
                        from: from.clone(),
                        to: to.clone(),
                        variable,
                    });
                }
            }
            Op::ExtractFunction {
                source, body_range, new_name, moved_text: serialized_moved, ..
            } => {
                let file = source
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("extract source missing file"))?
                    .to_string();
                let from_fn = extract_fn_name_from_path(source)?;
                // Prefer a snapshot serialized into the op — that's how
                // extract ops from a prior-generation merged log survive.
                // Fall back to reading the base_tree for first-gen merges.
                let moved_text = match serialized_moved {
                    Some(s) => s.clone(),
                    None => {
                        let base_text = base_tree
                            .get(&PathBuf::from(&file))
                            .ok_or_else(|| anyhow!("base missing {}", file))?;
                        snapshot_fn_body_stmts(base_text, &from_fn, *body_range)?
                    }
                };
                r.extractions.push(Extraction {
                    file,
                    from_fn,
                    to_fn: new_name.clone(),
                    moved_text,
                });
            }
            Op::WrapElement { anchor, .. } => {
                let file = anchor
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let selector = anchor
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if file.is_empty() || selector.is_empty() {
                    continue;
                }
                if let Some(base_text) = base_tree.get(&PathBuf::from(&file)) {
                    if let Some(children) = extract_element_children(base_text, &selector) {
                        r.wraps.push(WrapEntry {
                            file,
                            base_children: children,
                        });
                    }
                }
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

fn snapshot_fn_body_stmts(text: &str, fn_name: &str, range: [usize; 2]) -> Result<String> {
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
            let mut after = after;
            for rn in &r.symbol_renames {
                if rn.file == file {
                    source = rename_word_boundary(&source, &rn.old, &rn.new);
                    // The `after` anchor names a sibling function that A
                    // may have renamed — propagate.
                    if let Some(obj) = after.as_object_mut() {
                        if obj.get("name").and_then(|v| v.as_str()) == Some(rn.old.as_str()) {
                            obj.insert(
                                "name".to_string(),
                                serde_json::Value::String(rn.new.clone()),
                            );
                        }
                    }
                }
            }
            Op::AddFunction { anchor, after, name, source }
        }
        Op::InsertElement { anchor, position, html } => {
            let mut html = html;
            for (old, new) in &r.class_renames {
                html = rename_html_class_attr(&html, old, new);
            }
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
        Op::InsertAfter { anchor, content, reassigns } => {
            let file = anchor
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut anchor = anchor;
            let mut content = content;

            let ext = std::path::Path::new(&file)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let is_source_ext = matches!(ext, "js" | "ts" | "jsx" | "tsx" | "html" | "htm");

            // (1) Project-wide renames propagate to B's InsertAfter needle
            // and content. Without this, a needle referencing the pre-rename
            // symbol would fail to resolve against the post-A tree and any
            // inserted code would still reference the dead name.
            if is_source_ext {
                if let Some(needle) = anchor
                    .get("after_line_contains")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                {
                    let mut rewritten = needle;
                    for (old, new) in &r.project_symbol_renames {
                        rewritten = rename_word_boundary(&rewritten, old, new);
                    }
                    set_needle(&mut anchor, rewritten);
                }
                for (old, new) in &r.project_symbol_renames {
                    content = rename_word_boundary(&content, old, new);
                }
            }

            // (1b) Retype_binding propagates to needles that reference the
            // pre-retype declaration. `let foo = ...` → `const foo = ...`
            // means a needle containing `let foo` must become `const foo`.
            for rt in &r.retypes {
                if rt.file != file {
                    continue;
                }
                if let Some(needle) = anchor
                    .get("after_line_contains")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                {
                    let from_prefix = format!("{} {}", rt.from, rt.variable);
                    let to_prefix = format!("{} {}", rt.to, rt.variable);
                    if needle.contains(&from_prefix) {
                        let rewritten = needle.replace(&from_prefix, &to_prefix);
                        set_needle(&mut anchor, rewritten);
                    }
                }
            }

            // (2) Module-scope renames A made to THIS file also propagate
            // into needle and content (mirrors the AddFunction arm).
            for rn in &r.symbol_renames {
                if rn.file != file {
                    continue;
                }
                if let Some(needle) = anchor
                    .get("after_line_contains")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                {
                    let rewritten = rename_word_boundary(&needle, &rn.old, &rn.new);
                    set_needle(&mut anchor, rewritten);
                }
                content = rename_word_boundary(&content, &rn.old, &rn.new);
            }

            // (3) HTML wrap: if B's needle targets a line that A's
            // wrap_element moved into a new wrapper (children re-indented
            // +2), shift B's content by the same +2 so the new line lands
            // at the wrapped depth rather than at the old parent depth.
            let is_html = matches!(ext, "html" | "htm");
            if is_html {
                let needle = anchor
                    .get("after_line_contains")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !needle.is_empty() {
                    for w in &r.wraps {
                        if w.file != file {
                            continue;
                        }
                        if w.base_children.lines().any(|l| l.contains(&needle)) {
                            content = indent_nonblank_lines(&content, 2);
                            break;
                        }
                    }
                }
            }

            Op::InsertAfter { anchor, content, reassigns }
        }
        other => other,
    })
}

fn set_needle(anchor: &mut serde_json::Value, val: String) {
    if let Some(obj) = anchor.as_object_mut() {
        obj.insert(
            "after_line_contains".to_string(),
            serde_json::Value::String(val),
        );
    }
}

/// Return the verbatim text *between* the open and close tags of the
/// first `<selector>...</selector>` block in `text`, or None if not
/// found. Used only to probe "does B's needle point into this block?".
fn extract_element_children(text: &str, selector: &str) -> Option<String> {
    let open_pat = format!("<{}>", selector);
    let close_pat = format!("</{}>", selector);
    let open_idx = text.find(&open_pat)?;
    let after_open = open_idx + open_pat.len();
    let close_rel = text[after_open..].find(&close_pat)?;
    Some(text[after_open..after_open + close_rel].to_string())
}

/// Prepend `n` spaces to every non-blank line of `s`. Blank lines stay
/// blank so we don't sprinkle trailing whitespace.
fn indent_nonblank_lines(s: &str, n: usize) -> String {
    let pad: String = " ".repeat(n);
    s.split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{}{}", pad, line)
            }
        })
        .collect()
}

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

#[cfg(test)]
mod idempotence_tests {
    //! One test per dedup rule in `op_is_noop_against`. Each rule has a
    //! positive case (skip) and a negative case (still apply) to pin the
    //! state-based check against accidental over-/under-filtering.
    use super::*;
    use serde_json::json;

    fn tree_of(pairs: &[(&str, &str)]) -> Tree {
        pairs
            .iter()
            .map(|(p, t)| (PathBuf::from(p), t.to_string()))
            .collect()
    }

    #[test]
    fn add_import_skipped_when_already_present() {
        let tree = tree_of(&[("tool.py", "import os\nimport sys\n\nprint(1)\n")]);
        let op = Op::AddImport {
            anchor: json!({"file": "tool.py"}),
            module: "sys".into(),
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn add_import_applied_when_absent() {
        let tree = tree_of(&[("tool.py", "import os\n\nprint(1)\n")]);
        let op = Op::AddImport {
            anchor: json!({"file": "tool.py"}),
            module: "sys".into(),
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn delete_function_skipped_when_target_gone() {
        let tree = tree_of(&[("svc.py", "def keep():\n    pass\n")]);
        let op = Op::DeleteFunction {
            anchor: json!({
                "file": "svc.py",
                "path": ["module", {"kind": "function_def", "name": "old_fn"}]
            }),
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn delete_function_applied_when_target_still_present() {
        let tree = tree_of(&[("svc.py", "def old_fn():\n    pass\n")]);
        let op = Op::DeleteFunction {
            anchor: json!({
                "file": "svc.py",
                "path": ["module", {"kind": "function_def", "name": "old_fn"}]
            }),
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn replace_in_anchor_skipped_when_a_already_made_the_same_edit() {
        let tree = tree_of(&[("greet.py", "def greet():\n    print(\"Hello\")\n")]);
        let op = Op::ReplaceInAnchor {
            anchor: json!({
                "file": "greet.py",
                "path": ["module", {"kind": "function_def", "name": "greet"}, "body"]
            }),
            old: "Helo".into(),
            new: "Hello".into(),
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn replace_in_anchor_applied_when_old_still_there() {
        let tree = tree_of(&[("greet.py", "def greet():\n    print(\"Helo\")\n")]);
        let op = Op::ReplaceInAnchor {
            anchor: json!({
                "file": "greet.py",
                "path": ["module", {"kind": "function_def", "name": "greet"}, "body"]
            }),
            old: "Helo".into(),
            new: "Hello".into(),
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn rename_symbol_module_skipped_when_old_name_gone() {
        let tree = tree_of(&[("db.py", "def connect():\n    pass\n")]);
        let op = Op::RenameSymbol {
            scope: "module".into(),
            file: "db.py".into(),
            old: "get_conn".into(),
            new: "connect".into(),
            kinds: vec!["identifier".into()],
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn rename_symbol_module_applied_when_old_name_present() {
        let tree = tree_of(&[("db.py", "def get_conn():\n    pass\n")]);
        let op = Op::RenameSymbol {
            scope: "module".into(),
            file: "db.py".into(),
            old: "get_conn".into(),
            new: "connect".into(),
            kinds: vec!["identifier".into()],
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn rename_css_class_skipped_when_old_class_gone() {
        let tree = tree_of(&[
            ("style.css", ".btn-new { color: red; }\n"),
            ("index.html", "<div class=\"btn-new\"></div>\n"),
        ]);
        let op = Op::RenameCssClass {
            scope: "project".into(),
            old: "btn-old".into(),
            new: "btn-new".into(),
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn rename_css_class_applied_when_old_class_present() {
        let tree = tree_of(&[("style.css", ".btn-old { color: red; }\n")]);
        let op = Op::RenameCssClass {
            scope: "project".into(),
            old: "btn-old".into(),
            new: "btn-new".into(),
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn sort_block_skipped_when_already_sorted() {
        let tree = tree_of(&[("main.py", "import a\nimport b\nimport c\n\nprint(1)\n")]);
        let op = Op::SortBlock {
            anchor: json!({"file": "main.py", "path": ["module", "imports"]}),
            key: "alphabetic".into(),
        };
        assert!(op_is_noop_against(&op, &tree).unwrap());
    }

    #[test]
    fn sort_block_applied_when_unsorted() {
        let tree = tree_of(&[("main.py", "import c\nimport a\nimport b\n\nprint(1)\n")]);
        let op = Op::SortBlock {
            anchor: json!({"file": "main.py", "path": ["module", "imports"]}),
            key: "alphabetic".into(),
        };
        assert!(!op_is_noop_against(&op, &tree).unwrap());
    }
}
