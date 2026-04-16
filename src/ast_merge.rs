//! AST-aware 3-way merge for supported languages. Instead of line-level
//! diff3, parse all three versions (base, ours, theirs) with tree-sitter,
//! extract top-level items by name, and merge at the item level. Items
//! that only one side touched → take that side cleanly. Items both sides
//! touched → narrow diff3 to just that item's text (much less likely to
//! collide than whole-file diff3).
//!
//! This directly fixes the "every dogfood merge corrupts src/cli.rs"
//! pattern where diff3 silently drops braces / function declarations when
//! two branches edit different functions in the same large file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// A named top-level item extracted from a source file's AST.
#[derive(Debug, Clone)]
struct Item {
    /// (kind, name) tuple for identity. E.g. ("function_item", "run_git_merge").
    kind: String,
    name: String,
    /// The full source text of the item (including leading whitespace on
    /// its first line, trailing newline). Byte-exact from the source.
    text: String,
}

impl Item {
    fn key(&self) -> (String, String) {
        (self.kind.clone(), self.name.clone())
    }
}

/// Result of an AST-aware 3-way merge.
pub enum AstMergeResult {
    /// Clean merge; the merged text is ready to write.
    Clean(String),
    /// At least one top-level item had a per-item diff3 conflict.
    /// The merged text contains conflict markers for the user.
    Conflict(String),
    /// AST merge not applicable (unsupported language, parse failure,
    /// no named items). Caller should fall back to raw diff3.
    NotApplicable,
}

/// Try an AST-aware 3-way merge of `base`, `ours`, `theirs` for the
/// file at `path`. Returns `NotApplicable` when the file's language
/// isn't supported or the parse fails.
pub fn ast_merge(path: &Path, base: &str, ours: &str, theirs: &str) -> AstMergeResult {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang: tree_sitter::Language = match ext {
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        _ => return AstMergeResult::NotApplicable,
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return AstMergeResult::NotApplicable;
    }
    let Some(base_tree) = parser.parse(base, None) else {
        return AstMergeResult::NotApplicable;
    };
    let Some(ours_tree) = parser.parse(ours, None) else {
        return AstMergeResult::NotApplicable;
    };
    let Some(theirs_tree) = parser.parse(theirs, None) else {
        return AstMergeResult::NotApplicable;
    };
    // Refuse to merge if any input has parse errors — the item
    // extraction would be unreliable.
    if base_tree.root_node().has_error()
        || ours_tree.root_node().has_error()
        || theirs_tree.root_node().has_error()
    {
        return AstMergeResult::NotApplicable;
    }

    let base_items = extract_items(base_tree.root_node(), base.as_bytes(), ext);
    let ours_items = extract_items(ours_tree.root_node(), ours.as_bytes(), ext);
    let theirs_items = extract_items(theirs_tree.root_node(), theirs.as_bytes(), ext);

    // If no named items in any version, the file is probably not
    // structured enough for item-level merge.
    if base_items.is_empty() && ours_items.is_empty() && theirs_items.is_empty() {
        return AstMergeResult::NotApplicable;
    }

    merge_items(&base_items, &ours_items, &theirs_items)
}

/// Extract named top-level items from the root node. Items are
/// identified by (kind, name); unnamed nodes (bare expressions,
/// comments between items, module docstrings) are captured as
/// `("__anon__", "__NNN__")` with a sequential index so they
/// participate in ordering but don't match across versions by name.
fn extract_items(root: tree_sitter::Node, source: &[u8], ext: &str) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut anon_idx = 0usize;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind().to_string();
        let text = std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
            .unwrap_or("")
            .to_string();
        let name = item_name(child, source, ext);
        match name {
            Some(n) => items.push(Item { kind, name: n, text }),
            None => {
                // Anonymous item — assign a positional key.
                items.push(Item {
                    kind: "__anon__".to_string(),
                    name: format!("__{anon_idx}__"),
                    text,
                });
                anon_idx += 1;
            }
        }
    }
    items
}

/// Try to extract a name for a top-level AST node.
fn item_name(node: tree_sitter::Node, source: &[u8], ext: &str) -> Option<String> {
    // Common pattern: node has a `name` field.
    if let Some(name_node) = node.child_by_field_name("name") {
        return node_text(name_node, source);
    }
    // Language-specific fallback patterns.
    match ext {
        "py" => {
            // import_statement: `import X` → name = "import X"
            // expression_statement: first child identifier (assignments).
            match node.kind() {
                "import_statement" | "import_from_statement" => {
                    return Some(node_text_raw(node, source).trim().to_string());
                }
                "expression_statement" => {
                    // `X = ...` → name = "X"
                    let mut c = node.walk();
                    for child in node.children(&mut c) {
                        if child.kind() == "assignment" {
                            if let Some(left) = child.child_by_field_name("left") {
                                return node_text(left, source);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        "rs" => {
            // use_declaration: `use foo::bar;` → full text as name.
            if node.kind() == "use_declaration" {
                return Some(node_text_raw(node, source).trim().to_string());
            }
        }
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => {
            // export_statement wraps a declaration; dig into it.
            if node.kind() == "export_statement" {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if let Some(n) = child.child_by_field_name("name") {
                        return node_text(n, source);
                    }
                }
            }
            // import_statement: full text as name.
            if node.kind() == "import_statement" {
                return Some(node_text_raw(node, source).trim().to_string());
            }
            // lexical_declaration / variable_declaration: first declarator's name.
            if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "variable_declarator" {
                        if let Some(n) = child.child_by_field_name("name") {
                            return node_text(n, source);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    None
}

fn node_text(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
        .ok()
        .map(|s| s.to_string())
}

fn node_text_raw(node: tree_sitter::Node, source: &[u8]) -> String {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()])
        .unwrap_or("")
        .to_string()
}

/// 3-way merge at item granularity.
fn merge_items(base: &[Item], ours: &[Item], theirs: &[Item]) -> AstMergeResult {
    let base_map: BTreeMap<(String, String), &str> =
        base.iter().map(|i| (i.key(), i.text.as_str())).collect();
    let ours_map: BTreeMap<(String, String), &str> =
        ours.iter().map(|i| (i.key(), i.text.as_str())).collect();
    let theirs_map: BTreeMap<(String, String), &str> =
        theirs.iter().map(|i| (i.key(), i.text.as_str())).collect();

    // Determine the union of all item keys, ordered by their position
    // in `ours` first, then any theirs-only items appended at the
    // position they'd appear relative to shared neighbors.
    let mut order: Vec<(String, String)> = Vec::new();
    let ours_keys: Vec<(String, String)> = ours.iter().map(|i| i.key()).collect();
    let theirs_keys: Vec<(String, String)> = theirs.iter().map(|i| i.key()).collect();
    let ours_set: BTreeSet<(String, String)> = ours_keys.iter().cloned().collect();

    // Start with ours's order.
    for key in &ours_keys {
        order.push(key.clone());
    }
    // Append theirs-only items at the end (simple; positional
    // interleaving is possible but complex and rarely needed).
    for key in &theirs_keys {
        if !ours_set.contains(key) {
            order.push(key.clone());
        }
    }
    // Deduplicate (anonymous items with same key in both sides).
    let mut seen = BTreeSet::new();
    order.retain(|k| seen.insert(k.clone()));

    let mut merged_parts: Vec<String> = Vec::new();
    let mut has_conflict = false;

    for key in &order {
        let in_base = base_map.get(key).copied();
        let in_ours = ours_map.get(key).copied();
        let in_theirs = theirs_map.get(key).copied();

        let merged_text = match (in_base, in_ours, in_theirs) {
            // Only in ours (added by ours, or deleted by theirs).
            (None, Some(o), None) => o.to_string(),
            (Some(_), Some(o), None) => {
                // In base + ours but theirs deleted it. Take theirs's
                // decision (delete). Skip this item.
                let _ = o;
                continue;
            }
            // Only in theirs (added by theirs, or deleted by ours).
            (None, None, Some(t)) => t.to_string(),
            (Some(_), None, Some(_)) => {
                // In base + theirs but ours deleted it. Skip.
                continue;
            }
            // In base only (both deleted). Skip.
            (Some(_), None, None) => continue,
            // Not in any (shouldn't happen). Skip.
            (None, None, None) => continue,
            // In all three.
            (Some(b), Some(o), Some(t)) => {
                if o == t {
                    // Both sides agree (or neither changed).
                    o.to_string()
                } else if o == b {
                    // Only theirs changed.
                    t.to_string()
                } else if t == b {
                    // Only ours changed.
                    o.to_string()
                } else {
                    // Both changed differently. Narrow diff3 to this item.
                    match diff3_item(o, b, t) {
                        Ok(merged) => {
                            if merged.lines().any(|l| l.starts_with("<<<<<<<")) {
                                has_conflict = true;
                            }
                            merged
                        }
                        Err(_) => {
                            has_conflict = true;
                            format!(
                                "<<<<<<< ours\n{}\n=======\n{}\n>>>>>>> theirs\n",
                                o, t
                            )
                        }
                    }
                }
            }
            // New on both sides — convergent or divergent.
            (None, Some(o), Some(t)) => {
                if o == t {
                    o.to_string()
                } else {
                    // Both sides added an item with the same key but
                    // different content. Conflict.
                    has_conflict = true;
                    format!(
                        "<<<<<<< ours\n{}\n=======\n{}\n>>>>>>> theirs\n",
                        o, t
                    )
                }
            }
        };
        merged_parts.push(merged_text);
    }

    // Reassemble with double-newline separators (common convention).
    // Trim trailing whitespace from each part to normalize, then rejoin.
    let mut output = String::new();
    for (i, part) in merged_parts.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(part.trim_end());
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }

    if has_conflict {
        AstMergeResult::Conflict(output)
    } else {
        AstMergeResult::Clean(output)
    }
}

/// Run diff3 on a single item's text. Uses tmpfiles like the
/// whole-file diff3 path. Inputs are newline-normalized (diff3 on
/// macOS errors with "logic error" on files without trailing
/// newlines; tree-sitter node spans often omit them).
fn diff3_item(ours: &str, base: &str, theirs: &str) -> Result<String, String> {
    let ensure_nl = |s: &str| -> String {
        if s.ends_with('\n') { s.to_string() } else { format!("{}\n", s) }
    };
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tag = format!("llm-edit-ast-{}-{}", pid, nanos);
    let ours_p = dir.join(format!("{}-ours", tag));
    let base_p = dir.join(format!("{}-base", tag));
    let theirs_p = dir.join(format!("{}-theirs", tag));
    std::fs::write(&ours_p, ensure_nl(ours)).map_err(|e| e.to_string())?;
    std::fs::write(&base_p, ensure_nl(base)).map_err(|e| e.to_string())?;
    std::fs::write(&theirs_p, ensure_nl(theirs)).map_err(|e| e.to_string())?;
    let out = Command::new("diff3")
        .arg("-m")
        .arg(&ours_p)
        .arg(&base_p)
        .arg(&theirs_p)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&ours_p);
    let _ = std::fs::remove_file(&base_p);
    let _ = std::fs::remove_file(&theirs_p);
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
