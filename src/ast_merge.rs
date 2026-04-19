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
    /// Whitespace from the previous item's end (or the container's
    /// opening) up to this item's start. Carried so reassembly can
    /// concatenate items and reproduce the original separator
    /// structure (one blank line between functions, no blank lines
    /// between adjacent doc-comment lines, etc.) instead of
    /// stamping a hardcoded "\n\n" between every pair.
    leading: String,
    /// (kind, name) tuple for identity. E.g. ("function_item", "run_git_merge").
    kind: String,
    name: String,
    /// The item's source text from `start_byte()` to `end_byte()`,
    /// without leading or trailing whitespace.
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
        "css" => tree_sitter_css::LANGUAGE.into(),
        "html" | "htm" => tree_sitter_html::LANGUAGE.into(),
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

    merge_items(&base_items, &ours_items, &theirs_items, ext)
}

/// Extract named top-level items from the root node. Anonymous nodes
/// (line_comment doc blocks, attribute_item attributes, etc.) are
/// NOT promoted to items — their bytes get absorbed into the next
/// named item's `leading` so doc comments stay attached to the
/// function they document.
///
/// Without this, anonymous trivia would get positional anon keys
/// (`__0__`, `__1__`, ...) that collide between sides whenever each
/// side adds different leading trivia (a new function with its own
/// doc comment), and the merger conflict-merges them into duplicates
/// or orphaned-comment garbage.
fn extract_items(root: tree_sitter::Node, source: &[u8], ext: &str) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut prev_named_end = 0usize;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let name = match item_name(child, source, ext) {
            Some(n) => n,
            None => continue, // trivia: bytes ride along in next leading
        };
        let leading = std::str::from_utf8(&source[prev_named_end..child.start_byte()])
            .unwrap_or("")
            .to_string();
        let text = std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
            .unwrap_or("")
            .to_string();
        prev_named_end = child.end_byte();
        let kind = child.kind().to_string();
        items.push(Item { leading, kind, name, text });
    }
    // Capture any trailing bytes after the last named item (typically
    // just a final newline; occasionally a trailing trivia comment).
    // Carry them as a synthetic anon item so reassembly preserves
    // file-ending whitespace exactly.
    let src_len = source.len();
    if prev_named_end < src_len {
        let trailing = std::str::from_utf8(&source[prev_named_end..src_len])
            .unwrap_or("")
            .to_string();
        if !trailing.trim().is_empty() {
            // Real trailing content — keep it as an anon item.
            items.push(Item {
                leading: String::new(),
                kind: "__trailing__".to_string(),
                name: "__trailing__".to_string(),
                text: trailing,
            });
        }
        // Pure-whitespace trailing is handled by merge_items's final
        // newline guarantee; don't bother emitting an item for it.
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

/// 3-way merge at item granularity. Each emitted item carries its
/// original leading whitespace (captured at extract time) so the
/// output reproduces the source's separator structure between items
/// — adjacent doc-comment lines stay tight, adjacent functions keep
/// their blank line, etc. Hardcoded separators would corrupt the
/// former case.
fn merge_items(base: &[Item], ours: &[Item], theirs: &[Item], ext: &str) -> AstMergeResult {
    let base_map: BTreeMap<(String, String), &Item> =
        base.iter().map(|i| (i.key(), i)).collect();
    let ours_map: BTreeMap<(String, String), &Item> =
        ours.iter().map(|i| (i.key(), i)).collect();
    let theirs_map: BTreeMap<(String, String), &Item> =
        theirs.iter().map(|i| (i.key(), i)).collect();

    // Determine the union of all item keys, ordered by their position
    // in `ours` first, then any theirs-only items appended at the end.
    let mut order: Vec<(String, String)> = Vec::new();
    let ours_keys: Vec<(String, String)> = ours.iter().map(|i| i.key()).collect();
    let theirs_keys: Vec<(String, String)> = theirs.iter().map(|i| i.key()).collect();
    let ours_set: BTreeSet<(String, String)> = ours_keys.iter().cloned().collect();
    for key in &ours_keys {
        order.push(key.clone());
    }
    for key in &theirs_keys {
        if !ours_set.contains(key) {
            order.push(key.clone());
        }
    }
    let mut seen = BTreeSet::new();
    order.retain(|k| seen.insert(k.clone()));

    let mut output = String::new();
    let mut has_conflict = false;

    for key in &order {
        let in_base = base_map.get(key).copied();
        let in_ours = ours_map.get(key).copied();
        let in_theirs = theirs_map.get(key).copied();

        // Decide: which side's leading whitespace, and what content.
        let (leading_src, content): (&str, String) = match (in_base, in_ours, in_theirs) {
            // Only in ours (added by ours, or deleted by theirs).
            (None, Some(o), None) => (o.leading.as_str(), o.text.clone()),
            (Some(_), Some(_), None) => continue, // theirs deleted; skip
            // Only in theirs (added by theirs, or deleted by ours).
            (None, None, Some(t)) => (t.leading.as_str(), t.text.clone()),
            (Some(_), None, Some(_)) => continue, // ours deleted; skip
            (Some(_), None, None) => continue,
            (None, None, None) => continue,
            (Some(b), Some(o), Some(t)) => {
                let content = if o.text == t.text {
                    o.text.clone()
                } else if o.text == b.text {
                    // Only theirs changed.
                    t.text.clone()
                } else if t.text == b.text {
                    // Only ours changed.
                    o.text.clone()
                } else {
                    // Both changed differently. Try recursive AST merge
                    // first (e.g. two branches add different methods to
                    // the same impl block); fall back to diff3.
                    match merge_changed_item_recursive(&o.text, &b.text, &t.text, ext) {
                        Some((merged, conflicted)) => {
                            if conflicted {
                                has_conflict = true;
                            }
                            merged
                        }
                        None => match diff3_item(&o.text, &b.text, &t.text) {
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
                                    o.text, t.text
                                )
                            }
                        },
                    }
                };
                (o.leading.as_str(), content)
            }
            (None, Some(o), Some(t)) => {
                let content = if o.text == t.text {
                    o.text.clone()
                } else {
                    has_conflict = true;
                    format!(
                        "<<<<<<< ours\n{}\n=======\n{}\n>>>>>>> theirs\n",
                        o.text, t.text
                    )
                };
                (o.leading.as_str(), content)
            }
        };

        // Each item carries its captured leading (whitespace from the
        // previous item's end). The first emitted item's leading is
        // whatever sat at the start of the source (often empty).
        output.push_str(leading_src);
        output.push_str(&content);
    }

    // Trailing newline: most files end with a single newline. If the
    // last item's text already ends in one, leave it; otherwise add one.
    if !output.is_empty() && !output.ends_with('\n') {
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

/// Try to merge an item that both sides changed by recursing into its
/// named-children container (e.g. an impl block's method list, a
/// class body). Returns `Some((merged_text, has_conflict))` on success;
/// `None` when the item has no recognizable inner container, in which
/// case the caller should fall back to whole-item diff3.
///
/// Limited to containers whose children don't need separators between
/// them (Rust `impl_item` / `mod_item`, Python class body, JS/TS class
/// body). Struct/enum support would need separator-aware reassembly
/// and is left for follow-up.
fn merge_changed_item_recursive(
    ours: &str,
    base: &str,
    theirs: &str,
    ext: &str,
) -> Option<(String, bool)> {
    let lang: tree_sitter::Language = match ext {
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "css" => tree_sitter_css::LANGUAGE.into(),
        "html" | "htm" => tree_sitter_html::LANGUAGE.into(),
        _ => return None,
    };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).ok()?;

    let tree_o = parser.parse(ours, None)?;
    let tree_b = parser.parse(base, None)?;
    let tree_t = parser.parse(theirs, None)?;
    if tree_o.root_node().has_error()
        || tree_b.root_node().has_error()
        || tree_t.root_node().has_error()
    {
        return None;
    }

    // Each side's text should be a single top-level item.
    let item_o = tree_o.root_node().child(0)?;
    let item_b = tree_b.root_node().child(0)?;
    let item_t = tree_t.root_node().child(0)?;
    if item_o.kind() != item_b.kind() || item_o.kind() != item_t.kind() {
        return None;
    }

    let body_o = inner_container(item_o, ext)?;
    let body_b = inner_container(item_b, ext)?;
    let body_t = inner_container(item_t, ext)?;

    let (sub_o, trail_o) = extract_subitems(body_o, ours.as_bytes(), ext);
    let (sub_b, _) = extract_subitems(body_b, base.as_bytes(), ext);
    let (sub_t, _) = extract_subitems(body_t, theirs.as_bytes(), ext);

    // No named children means no benefit from recursion.
    if sub_o.is_empty() && sub_b.is_empty() && sub_t.is_empty() {
        return None;
    }

    let (merged_subs_text, has_conflict) = match merge_items(&sub_b, &sub_o, &sub_t, ext) {
        AstMergeResult::Clean(text) => (text, false),
        AstMergeResult::Conflict(text) => (text, true),
        AstMergeResult::NotApplicable => return None,
    };

    // Splice merged inner content into ours's frame: keep everything up
    // to and including the body's opening brace, then the merged subs
    // (their leading whitespace already includes the post-brace newline +
    // indentation), then ours's trailing whitespace before `}`, then `}`
    // and anything after.
    let body_start = body_o.start_byte();
    let body_end = body_o.end_byte();
    if body_end == 0 || body_end <= body_start {
        return None;
    }
    let prefix = &ours[..body_start + 1]; // include `{`
    let suffix = &ours[body_end - 1..]; // start at `}`
    // merged_subs_text from merge_items ends with a single trailing
    // newline; strip it so trail_o (which is whatever ours had between
    // the last sub-item and `}`) governs.
    let middle = format!(
        "{}{}",
        merged_subs_text.strip_suffix('\n').unwrap_or(&merged_subs_text),
        trail_o
    );

    Some((format!("{}{}{}", prefix, middle, suffix), has_conflict))
}

/// Find the named-children container body of a top-level item, if any.
/// Only returns containers wrapped in `{` and `}` so the splice in
/// `merge_changed_item_recursive` is uniform. Python class bodies have
/// no braces and need indent-aware splicing — left for follow-up.
fn inner_container<'a>(item: tree_sitter::Node<'a>, ext: &str) -> Option<tree_sitter::Node<'a>> {
    match ext {
        "rs" => match item.kind() {
            "impl_item" | "mod_item" => item.child_by_field_name("body"),
            _ => None,
        },
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => match item.kind() {
            "class_declaration" => item.child_by_field_name("body"),
            _ => None,
        },
        _ => None,
    }
}

/// Walk the children of a container node (impl body, class body, etc.)
/// and extract sub-items in the same shape as `extract_items`. Skips
/// structural delimiter tokens (braces, colons, indents) so the merge
/// reassembly doesn't double-stamp them. Returns the items plus the
/// trailing whitespace between the last item and the closing brace
/// (used by the splice to preserve indentation around `}`).
fn extract_subitems(
    body: tree_sitter::Node,
    source: &[u8],
    ext: &str,
) -> (Vec<Item>, String) {
    let mut items: Vec<Item> = Vec::new();
    // Start position right after the body's opening `{`.
    let mut prev_named_end = body.start_byte() + 1;
    let body_close = body.end_byte().saturating_sub(1); // position of `}`
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        // Skip structural tokens; their bytes are accounted for by the
        // splice's prefix/suffix.
        match child.kind() {
            "{" | "}" | "(" | ")" | ":" | ";" => continue,
            "indent" | "dedent" | "newline" => continue,
            _ => {}
        }
        let name = match item_name(child, source, ext) {
            Some(n) => n,
            // Anonymous nodes (doc comments, attributes, etc.) are NOT
            // promoted to items. Positional anon keys collide between
            // sides when each side adds different leading trivia (e.g.
            // doc comments) for its own new method, producing spurious
            // conflicts. Instead, leave their bytes in the source and
            // they will be absorbed into the *next* named item's
            // `leading`, which keeps the trivia attached to the item it
            // documents.
            None => continue,
        };
        let leading = std::str::from_utf8(&source[prev_named_end..child.start_byte()])
            .unwrap_or("")
            .to_string();
        let text = std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
            .unwrap_or("")
            .to_string();
        prev_named_end = child.end_byte();
        let kind = child.kind().to_string();
        items.push(Item { leading, kind, name, text });
    }
    let trailing = if prev_named_end <= body_close {
        std::str::from_utf8(&source[prev_named_end..body_close])
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };
    (items, trailing)
}
