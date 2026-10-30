pub mod ast_merge;
pub mod cli;
pub mod git_tree;
pub mod merge_logic;
pub mod ops;

use anyhow::{Context, Result, anyhow};
use ops::{Declaration, Op};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// In-memory working tree: relative path -> file contents.
pub type Tree = BTreeMap<PathBuf, String>;

pub fn load_tree(root: &Path) -> Result<Tree> {
    let mut tree = Tree::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !SKIP_DIRS.iter().any(|d| name == *d)
    }) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root)?.to_path_buf();
        // Gracefully skip non-UTF-8 files (binaries, images, compiled
        // artifacts that leaked past the directory skiplist). They can't
        // participate in text-based ops and erroring on them blocks
        // dogfood on any repo with even one such file.
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(e) => return Err(e).with_context(|| format!("read {}", entry.path().display())),
        };
        tree.insert(rel, text);
    }
    Ok(tree)
}

pub fn apply_log(tree: &mut Tree, ops: &[Op]) -> Result<()> {
    for (i, op) in ops.iter().enumerate() {
        apply_op(tree, op).with_context(|| format!("applying op #{}", i + 1))?;
    }
    Ok(())
}

fn apply_op(tree: &mut Tree, op: &Op) -> Result<()> {
    match op {
        Op::RenameSymbol {
            scope,
            file,
            old,
            new,
            respect_shadowing,
            ..
        } => {
            match scope.as_str() {
                "module" => {
                    let path = PathBuf::from(file);
                    let text = tree
                        .get_mut(&path)
                        .ok_or_else(|| anyhow!("no such file {}", file))?;
                    *text = rename_in_source_file(text, &path, old, new, *respect_shadowing);
                }
                "project" => {
                    propagate_symbol_rename(tree, old, new, *respect_shadowing);
                }
                _ => return Err(anyhow!("unsupported rename_symbol scope: {}", scope)),
            }
            Ok(())
        }
        Op::RenameCssClass { scope, old, new } => {
            if scope != "project" {
                return Err(anyhow!("only scope=project supported in v0"));
            }
            propagate_css_class_rename(tree, old, new);
            Ok(())
        }
        Op::AddFunction {
            anchor,
            after,
            source,
            ..
        } => {
            let file = anchor_file(anchor)?;
            let after_name = after
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("add_function.after.name missing"))?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = insert_function_after(text, after_name, source)?;
            Ok(())
        }
        Op::InsertElement {
            anchor,
            position,
            html,
        } => {
            let file = anchor_file(anchor)?;
            let selector = anchor
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("insert_element.anchor.selector missing"))?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            match (selector, position.as_str()) {
                ("body", "last-child") => {
                    *text = insert_html_before_body_close(text, html);
                }
                (tag, "after") => {
                    *text = insert_html_after_tag(text, tag, html)?;
                }
                _ => {
                    return Err(anyhow!(
                        "v0 insert_element: unsupported selector={} position={}",
                        selector,
                        position
                    ));
                }
            }
            Ok(())
        }
        Op::ExtractFunction {
            source,
            body_range,
            new_name,
            params,
            returns,
            ..
        } => {
            let file = anchor_file(source)?;
            let fn_name = anchor_path_fn_name(source)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = extract_function(text, &fn_name, *body_range, new_name, params, returns)?;
            Ok(())
        }
        Op::DeleteNode { anchor } => {
            let file = anchor_file(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            // New content-hash anchor form: {file, node_id, parent_fn, text}.
            // The applier locates the matching comment by (parent_fn, normalized text)
            // regardless of its positional index in the fn body. This survives
            // a concurrent insert-before that would have drifted the positional
            // `{kind: "comment", index: N}` anchor.
            if anchor.get("node_id").is_some() {
                let parent_fn = anchor
                    .get("parent_fn")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("delete_node by node_id: anchor.parent_fn missing"))?;
                let target = anchor
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("delete_node by node_id: anchor.text missing"))?;
                *text = delete_comment_by_text_in_fn(text, parent_fn, target)?;
                return Ok(());
            }
            // Positional path (backward compatible).
            let fn_name = anchor_path_fn_name(anchor)?;
            let (kind, idx) = anchor_path_last_kind_index(anchor)?;
            if kind != "comment" {
                return Err(anyhow!("delete_node v0 only supports kind=comment"));
            }
            *text = delete_nth_comment_in_fn(text, &fn_name, idx)?;
            Ok(())
        }
        Op::ReplaceInAnchor { anchor, old, new } => {
            let file = anchor_file(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            if anchor.get("selector").is_some() {
                // HTML/CSS selector anchor: v0 does a file-wide substring
                // replace. Fine for our fixtures where `old` is unique.
                let pos = text
                    .find(old.as_str())
                    .ok_or_else(|| anyhow!("replace_in_anchor: `{}` not found", old))?;
                let mut out = String::with_capacity(text.len() + new.len());
                out.push_str(&text[..pos]);
                out.push_str(new);
                out.push_str(&text[pos + old.len()..]);
                *text = out;
            } else if let Ok(fn_name) = anchor_path_fn_name(anchor) {
                // Function-scoped replace.
                *text = replace_in_fn_body(text, &fn_name, old, new)?;
            } else {
                // File-scoped replace (no function in anchor path).
                let pos = text.find(old.as_str()).ok_or_else(|| {
                    anyhow!("replace_in_anchor (file-scoped): `{}` not found", old)
                })?;
                let mut out = String::with_capacity(text.len() + new.len());
                out.push_str(&text[..pos]);
                out.push_str(new);
                out.push_str(&text[pos + old.len()..]);
                *text = out;
            }
            Ok(())
        }
        Op::AddRule {
            anchor,
            selector,
            declarations,
        } => {
            let file = anchor_file(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = append_css_rule(text, selector, declarations);
            Ok(())
        }
        Op::SortBlock { anchor, key } => {
            if key != "alphabetic" {
                return Err(anyhow!("sort_block v0 only supports key=alphabetic"));
            }
            let file = anchor_file(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = sort_imports_alphabetic(text);
            Ok(())
        }
        Op::AddImport { anchor, module } => {
            let file = anchor_file(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = add_import_smart(text, module);
            Ok(())
        }
        Op::WrapElement { anchor, wrapper } => {
            let file = anchor_file(anchor)?;
            let selector = anchor
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("wrap_element.anchor.selector missing"))?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = wrap_element_children(text, selector, wrapper)?;
            Ok(())
        }
        Op::RetypeBinding {
            anchor, from, to, ..
        } => {
            let file = anchor_file(anchor)?;
            let variable = anchor
                .get("variable")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("retype_binding.anchor.variable missing"))?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = retype_binding_textual(text, variable, from, to)?;
            Ok(())
        }
        Op::DeleteFunction { anchor } => {
            let file = anchor_file(anchor)?;
            let fn_name = anchor_path_fn_name(anchor)?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = delete_top_level_fn(text, &fn_name)?;
            Ok(())
        }
        Op::InsertAfter {
            anchor, content, ..
        } => {
            let file = anchor_file(anchor)?;
            let needle = anchor
                .get("after_line_contains")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("insert_after.anchor.after_line_contains missing"))?;
            let text = tree
                .get_mut(&file)
                .ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = insert_line_after_matching(text, needle, content)?;
            Ok(())
        }
        Op::AddFile { file, content } => {
            let path = PathBuf::from(file);
            if tree.contains_key(&path) {
                return Err(anyhow!("add_file: {} already exists", file));
            }
            tree.insert(path, content.clone());
            Ok(())
        }
        Op::DeleteFile { file } => {
            let path = PathBuf::from(file);
            if tree.remove(&path).is_none() {
                return Err(anyhow!("delete_file: {} not found", file));
            }
            Ok(())
        }
        Op::MoveFile { from, to } => {
            let from_path = PathBuf::from(from);
            let to_path = PathBuf::from(to);
            let content = tree
                .remove(&from_path)
                .ok_or_else(|| anyhow!("move_file: {} not found", from))?;
            if tree.contains_key(&to_path) {
                return Err(anyhow!("move_file: target {} already exists", to));
            }
            tree.insert(to_path, content);
            Ok(())
        }
        Op::ChangeSignature {
            file,
            fn_name,
            add_params,
            remove_params,
        } => {
            let path = PathBuf::from(file);
            let text = tree
                .get_mut(&path)
                .ok_or_else(|| anyhow!("change_signature: {} not found", file))?;
            *text = change_fn_signature(text, fn_name, add_params, remove_params)?;
            Ok(())
        }
    }
}

/// Rewrite a function's parameter list: remove named params and append
/// new ones. Works for Python (`def f(a, b):`) and Rust (`fn f(a: i32,
/// b: bool)`) — both use `(...)` delimited param lists.
fn change_fn_signature(
    source: &str,
    fn_name: &str,
    add_params: &[String],
    remove_params: &[String],
) -> Result<String> {
    // Find `fn_name(` — the opening paren of the function signature.
    let needle = format!("{}(", fn_name);
    let Some(name_pos) = source.find(&needle) else {
        return Err(anyhow!(
            "change_signature: function `{}` not found",
            fn_name
        ));
    };
    let open = name_pos + fn_name.len(); // position of `(`
    // Find matching `)` — simple scan (no nested parens in param lists
    // for the v0 languages we support).
    let rest = &source[open..];
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close.ok_or_else(|| anyhow!("change_signature: unmatched `(` for {}", fn_name))?;
    let params_str = &source[open + 1..close]; // between ( and )
    // Parse existing params: split on `,`, trim each.
    let mut params: Vec<String> = if params_str.trim().is_empty() {
        Vec::new()
    } else {
        params_str
            .split(',')
            .map(|p| p.trim().to_string())
            .collect()
    };
    // Remove params by name (match the param name, which is the part
    // before `:` for typed langs or the whole token for Python).
    for rm in remove_params {
        params.retain(|p| {
            let name_part = p.split(&[':', '='][..]).next().unwrap_or(p).trim();
            // Also strip leading &, &mut, self qualifiers for Rust.
            let clean = name_part
                .trim_start_matches("&mut ")
                .trim_start_matches("&")
                .trim_start_matches("mut ")
                .trim();
            clean != rm.as_str()
        });
    }
    // Append new params.
    for add in add_params {
        if !params.iter().any(|p| {
            let name_part = p.split(&[':', '='][..]).next().unwrap_or(p).trim();
            let clean = name_part
                .trim_start_matches("&mut ")
                .trim_start_matches("&")
                .trim_start_matches("mut ")
                .trim();
            clean == add.split(':').next().unwrap_or(add).trim()
        }) {
            params.push(add.clone());
        }
    }
    let new_params = params.join(", ");
    let mut out = String::with_capacity(source.len() + new_params.len());
    out.push_str(&source[..open + 1]); // up to and including `(`
    out.push_str(&new_params);
    out.push_str(&source[close..]); // from `)` onward
    Ok(out)
}

// ----- anchor helpers ------------------------------------------------------

fn anchor_file(anchor: &serde_json::Value) -> Result<PathBuf> {
    anchor
        .get("file")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("anchor.file missing"))
}

/// Walk `anchor.path` and pull the first {kind: function_def, name: X} it finds.
fn anchor_path_fn_name(anchor: &serde_json::Value) -> Result<String> {
    let path = anchor
        .get("path")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("anchor.path missing or not array"))?;
    for seg in path {
        if let Some(obj) = seg.as_object() {
            if obj.get("kind").and_then(|v| v.as_str()) == Some("function_def") {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    return Ok(name.to_string());
                }
            }
        }
    }
    Err(anyhow!("no function_def segment in anchor.path"))
}

/// Read the last path segment as {kind, index}.
fn anchor_path_last_kind_index(anchor: &serde_json::Value) -> Result<(String, usize)> {
    let path = anchor
        .get("path")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("anchor.path missing"))?;
    let last = path.last().ok_or_else(|| anyhow!("anchor.path empty"))?;
    let obj = last
        .as_object()
        .ok_or_else(|| anyhow!("last path segment not object"))?;
    let kind = obj
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("last segment missing kind"))?
        .to_string();
    let index = obj
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("last segment missing index"))? as usize;
    Ok((kind, index))
}

// ----- textual heuristics --------------------------------------------------

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Dispatch identifier rename to the right AST backend for a given
/// source file. Unknown extensions fall back to textual word-boundary
/// rewrite.
fn rename_in_source_file(
    text: &str,
    path: &Path,
    old: &str,
    new: &str,
    respect_shadowing: bool,
) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "py" => py_ast_rename_identifier_scoped(text, old, new, respect_shadowing),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => {
            // JS/TS scope-aware rename tracked on #6 follow-up.
            js_ts_ast_rename_identifier(text, ext, old, new)
        }
        "rs" => rust_ast_rename_identifier(text, old, new),
        _ => rename_word_boundary(text, old, new),
    }
}

/// AST-scoped rename for Python: rewrite every `identifier` node whose
/// text equals `old` to `new`. String literal contents and comments are
/// untouched because they are not identifier nodes. Backwards-compatible
/// shape: ignores lexical shadowing (rewrites every match in the file).
///
/// Use `py_ast_rename_identifier_scoped` with `respect_shadowing=true`
/// to skip identifiers inside functions whose parameter list rebinds
/// the name.
pub fn py_ast_rename_identifier(source: &str, old: &str, new: &str) -> String {
    py_ast_rename_identifier_scoped(source, old, new, false)
}

/// Scope-aware Python rename. When `respect_shadowing` is true,
/// `function_definition` nodes whose `parameters` declares a name
/// matching `old` have their entire subtree skipped — the inner
/// parameter declaration AND every body reference resolve to that
/// inner binding (Python LEGB), so neither should be rewritten.
pub fn py_ast_rename_identifier_scoped(
    source: &str,
    old: &str,
    new: &str,
    respect_shadowing: bool,
) -> String {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_err()
    {
        return source.to_string();
    }
    let Some(tree) = parser.parse(source, None) else {
        return source.to_string();
    };
    let bytes = source.as_bytes();
    let old_b = old.as_bytes();

    let mut hits: Vec<(usize, usize)> = Vec::new();
    if respect_shadowing {
        collect_identifier_hits_scoped(tree.root_node(), bytes, old_b, &mut hits, false);
    } else {
        collect_identifier_hits(tree.root_node(), bytes, old_b, &mut hits);
    }
    hits.sort_unstable();
    hits.dedup();
    let mut out = source.to_string();
    for (s, e) in hits.iter().rev() {
        out.replace_range(*s..*e, new);
    }
    out
}

/// Scope-aware identifier walker (#6 MVP). Two-mode traversal:
///
/// - `in_target_scope = false`: we haven't yet entered the scope of
///   the binding being renamed. Recurse normally, BUT if we find a
///   `function_definition` whose parameter list declares `old`, that
///   function IS the binding's outermost scope. Recurse into it with
///   `in_target_scope = true` so we rewrite its parameter declaration
///   and body uses.
/// - `in_target_scope = true`: we are inside the target binding's
///   scope. Rewrite identifier matches. If we encounter a nested
///   `function_definition` that also declares `old` as a parameter,
///   that nested function shadows our binding — skip its entire
///   subtree.
///
/// Limits: only recognises function-parameter shadowing. for-loop /
/// with-as / class / `nonlocal` rebindings, and module-level top
/// assignments, aren't yet honored. See #6 for the full plan.
fn collect_identifier_hits_scoped(
    node: tree_sitter::Node,
    source: &[u8],
    old: &[u8],
    hits: &mut Vec<(usize, usize)>,
    in_target_scope: bool,
) {
    if node.kind() == "function_definition" && python_function_shadows(node, source, old) {
        if in_target_scope {
            // Nested fn that shadows our target binding — skip subtree.
            return;
        } else {
            // Outermost fn introducing the target binding — descend with
            // rename mode on so its parameter and body uses get rewritten.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_identifier_hits_scoped(child, source, old, hits, true);
            }
            return;
        }
    }
    if node.kind() == "identifier" {
        if in_target_scope {
            let s = node.start_byte();
            let e = node.end_byte();
            if &source[s..e] == old {
                hits.push((s, e));
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_hits_scoped(child, source, old, hits, in_target_scope);
    }
}

fn collect_identifier_hits(
    node: tree_sitter::Node,
    source: &[u8],
    old: &[u8],
    hits: &mut Vec<(usize, usize)>,
) {
    // Default (non-scope-aware) walker: rewrites every identifier
    // matching `old` regardless of which binding it refers to. The
    // scope-aware variant is `collect_identifier_hits_scoped`.
    if node.kind() == "identifier" {
        let s = node.start_byte();
        let e = node.end_byte();
        if &source[s..e] == old {
            hits.push((s, e));
        }
        return; // identifiers have no children worth scanning
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_hits(child, source, old, hits);
    }
}

/// True iff `fn_node` (a `function_definition`) declares a parameter
/// whose name matches `old`. Walks the `parameters` child; supports
/// bare `identifier`, `typed_parameter`, `default_parameter`,
/// `typed_default_parameter`, `list_splat_pattern` (*args), and
/// `dictionary_splat_pattern` (**kwargs).
fn python_function_shadows(fn_node: tree_sitter::Node, source: &[u8], old: &[u8]) -> bool {
    let Some(params) = fn_node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        let name_node = match child.kind() {
            "identifier" => Some(child),
            "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern" => {
                let mut c = child.walk();
                child.children(&mut c).find(|n| n.kind() == "identifier")
            }
            _ => None,
        };
        if let Some(n) = name_node {
            let s = n.start_byte();
            let e = n.end_byte();
            if &source[s..e] == old {
                return true;
            }
        }
    }
    false
}

/// AST-scoped rename for JavaScript / TypeScript (incl. JSX / TSX):
/// rewrite every identifier-kind node whose text equals `old` to `new`.
/// String-literal contents, template-string text (outside interpolation
/// holes), regex bodies, and comments are NOT renamed because they are
/// not identifier nodes in the grammar.
///
/// `ext` selects the parser:
///   - `"js" | "jsx" | "mjs" | "cjs"` → tree-sitter-javascript
///   - `"ts"` → tree-sitter-typescript (TypeScript grammar)
///   - `"tsx"` → tree-sitter-typescript (TSX grammar)
///
/// Unknown extensions return `source` unchanged.
pub fn js_ts_ast_rename_identifier(source: &str, ext: &str, old: &str, new: &str) -> String {
    let lang: tree_sitter::Language = match ext {
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        _ => return source.to_string(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return source.to_string();
    }
    let Some(tree) = parser.parse(source, None) else {
        return source.to_string();
    };
    let bytes = source.as_bytes();
    let old_b = old.as_bytes();

    let mut hits: Vec<(usize, usize)> = Vec::new();
    collect_js_identifier_hits(tree.root_node(), bytes, old_b, &mut hits);
    hits.sort_unstable();
    hits.dedup();
    let mut out = source.to_string();
    for (s, e) in hits.iter().rev() {
        out.replace_range(*s..*e, new);
    }
    out
}

/// AST-scoped rename for Rust source. Walks tree-sitter-rust's
/// identifier-kind nodes and rewrites only those — string literals,
/// raw strings, char literals, and comments are separate node kinds
/// in the grammar and are skipped by construction.
pub fn rust_ast_rename_identifier(source: &str, old: &str, new: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return source.to_string();
    }
    let Some(tree) = parser.parse(source, None) else {
        return source.to_string();
    };
    let bytes = source.as_bytes();
    let old_b = old.as_bytes();

    let mut hits: Vec<(usize, usize)> = Vec::new();
    collect_rust_identifier_hits(tree.root_node(), bytes, old_b, &mut hits);
    hits.sort_unstable();
    hits.dedup();
    let mut out = source.to_string();
    for (s, e) in hits.iter().rev() {
        out.replace_range(*s..*e, new);
    }
    out
}

fn collect_rust_identifier_hits(
    node: tree_sitter::Node,
    source: &[u8],
    old: &[u8],
    hits: &mut Vec<(usize, usize)>,
) {
    let kind = node.kind();
    // tree-sitter-rust identifier kinds: see grammar at
    // https://github.com/tree-sitter/tree-sitter-rust. Strings
    // (`string_literal`, `raw_string_literal`), char literals
    // (`char_literal`), comments (`line_comment`, `block_comment`),
    // and lifetimes (`lifetime`) are NOT identifier kinds and are
    // skipped automatically.
    let is_ident = matches!(
        kind,
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "shorthand_field_identifier"
            | "primitive_type" // matches when renaming e.g. type aliases
    );
    // Don't recurse into string / comment subtrees — their internal
    // node kinds may include identifier-shaped nodes (e.g. a macro
    // invocation inside a doc comment example) and we don't want
    // those clobbered.
    let is_text_subtree = matches!(
        kind,
        "string_literal"
            | "raw_string_literal"
            | "char_literal"
            | "line_comment"
            | "block_comment"
            | "doc_comment"
    );
    if is_text_subtree {
        return;
    }
    if is_ident {
        let s = node.start_byte();
        let e = node.end_byte();
        if &source[s..e] == old {
            hits.push((s, e));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_rust_identifier_hits(child, source, old, hits);
    }
}

fn collect_js_identifier_hits(
    node: tree_sitter::Node,
    source: &[u8],
    old: &[u8],
    hits: &mut Vec<(usize, usize)>,
) {
    let kind = node.kind();
    // The tree-sitter-javascript / tree-sitter-typescript grammars expose
    // a handful of identifier-flavoured node kinds. We rewrite every one
    // whose text matches `old`. Strings, template text, regex bodies, and
    // comments are OTHER node kinds (`string_fragment`, `template_chars`,
    // `comment`, …) and are skipped by design.
    let is_ident = matches!(
        kind,
        "identifier"
            | "property_identifier"
            | "shorthand_property_identifier"
            | "shorthand_property_identifier_pattern"
            | "type_identifier"
            | "jsx_identifier"
    );
    if is_ident {
        let s = node.start_byte();
        let e = node.end_byte();
        if &source[s..e] == old {
            hits.push((s, e));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_js_identifier_hits(child, source, old, hits);
    }
}

/// Replace every whole-word occurrence of `old` with `new`.
pub fn rename_word_boundary(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let old_bytes = old.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(old_bytes) {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1] as char);
            let after_i = i + old_bytes.len();
            let after_ok = after_i >= bytes.len() || !is_ident_char(bytes[after_i] as char);
            if before_ok && after_ok {
                out.push_str(new);
                i = after_i;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Replace `.old` → `.new` in CSS where it's a class selector token.
pub fn rename_css_selector_class(text: &str, old: &str, new: &str) -> String {
    // AST-backed: parse with tree-sitter-css, find class_selector nodes
    // whose class_name matches `old`, rewrite only those. Leaves comments,
    // strings (`content: ".old"`), and non-matching selectors untouched.
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .is_err()
    {
        // Fallback to text-based rename if parser setup fails.
        return rename_css_selector_class_text(text, old, new);
    }
    let Some(tree) = parser.parse(text, None) else {
        return rename_css_selector_class_text(text, old, new);
    };
    // Collect byte ranges of class names to rewrite (must collect first
    // to avoid invalidating positions during replacement).
    let mut replacements: Vec<(usize, usize)> = Vec::new();
    collect_css_class_hits(tree.root_node(), text.as_bytes(), old, &mut replacements);
    if replacements.is_empty() {
        return text.to_string();
    }
    // Apply replacements right-to-left to preserve earlier positions.
    let mut out = text.to_string();
    for &(start, end) in replacements.iter().rev() {
        out.replace_range(start..end, new);
    }
    out
}

/// In HTML, rename a class name inside every `class="..."` attribute value.
pub fn rename_html_class_attr(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        // look for `class="` or `class='`
        let Some(idx) = rest.find("class=") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..idx]);
        let after_eq = &rest[idx + "class=".len()..];
        let Some(quote) = after_eq.chars().next() else {
            out.push_str(&rest[idx..]);
            break;
        };
        if quote != '"' && quote != '\'' {
            out.push_str(&rest[idx..idx + "class=".len()]);
            rest = after_eq;
            continue;
        }
        let value_start = idx + "class=".len() + quote.len_utf8();
        let Some(end_rel) = rest[value_start..].find(quote) else {
            out.push_str(&rest[idx..]);
            break;
        };
        let value = &rest[value_start..value_start + end_rel];
        let rewritten = value
            .split_whitespace()
            .map(|tok| if tok == old { new } else { tok })
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("class={}{}{}", quote, rewritten, quote));
        rest = &rest[value_start + end_rel + quote.len_utf8()..];
    }
    out
}

/// Insert `source` as a new top-level function after the named function.
/// v0: only handles the case where the named function is the last top-level
/// statement; appends with PEP 8's two-blank-line separator.
fn insert_function_after(text: &str, after_name: &str, source: &str) -> Result<String> {
    let marker = format!("def {}(", after_name);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&marker))
        .ok_or_else(|| anyhow!("after function `{}` not found", after_name))?;
    let fn_indent = lines[def_idx].len() - lines[def_idx].trim_start().len();
    // Where to land the new function: after the target's body AND after
    // any sibling `def` at the same indent (so repeated "after X" inserts
    // stack in log-order, and cross-agent inserts preserve A-then-B), but
    // before any non-def top-level construct (imports at bottom, module
    // assignments like `_store = {}`) so trailing module-level code
    // stays where it was.
    let mut end = lines.len();
    for j in (def_idx + 1)..lines.len() {
        let line = lines[j];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        if is_blank {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent > fn_indent {
            continue; // inside some function's body
        }
        if indent == fn_indent && stripped.trim_start().starts_with("def ") {
            continue; // sibling def: skip past it
        }
        end = j;
        break;
    }
    // Gather head (up to and including the function's body), then compose
    // the insertion with PEP 8-style two-blank-line separator, then the
    // remainder of the file.
    let head: String = lines[..end].concat();
    let tail: String = lines[end..].concat();
    let mut out = String::with_capacity(text.len() + source.len() + 4);
    out.push_str(head.trim_end_matches('\n'));
    out.push_str("\n\n\n");
    out.push_str(source);
    if !tail.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        // Two blank lines between the inserted def and the following
        // top-level content (PEP 8 mirror of what we do on the leading side).
        out.push_str("\n\n");
        out.push_str(&tail);
    }
    Ok(out)
}

/// Insert `html` as the last child of `<body>`, matching sibling indent.
fn insert_html_before_body_close(text: &str, html: &str) -> String {
    let Some(close_idx) = text.find("</body>") else {
        return text.to_string();
    };
    // Find the indentation of the `</body>` line.
    let line_start = text[..close_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let close_indent: String = text[line_start..close_idx]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    // Child indent = close_indent + 2 spaces (matches our fixture's 2-space style).
    let child_indent = format!("{}  ", close_indent);
    let insertion = format!("{}{}\n", child_indent, html);
    let mut out = String::with_capacity(text.len() + insertion.len());
    out.push_str(&text[..line_start]);
    out.push_str(&insertion);
    out.push_str(&text[line_start..]);
    out
}

/// Delete the named top-level function and the blank lines that precede
/// it from a Python source file.
fn delete_top_level_fn(text: &str, fn_name: &str) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent = lines[def_idx].len() - lines[def_idx].trim_start().len();

    // End = first later line that is non-blank at indent <= fn_indent.
    let mut end = lines.len();
    for j in (def_idx + 1)..lines.len() {
        let line = lines[j];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent = line.len() - line.trim_start().len();
        if !is_blank && indent <= fn_indent {
            end = j;
            break;
        }
    }

    // Start = walk back over blank lines preceding the def.
    let mut start = def_idx;
    while start > 0 && lines[start - 1].trim().is_empty() {
        start -= 1;
    }

    let mut out = String::new();
    for (j, l) in lines.iter().enumerate() {
        if j < start || j >= end {
            out.push_str(l);
        }
    }
    Ok(out)
}

/// Change the declaration keyword for a named variable: e.g. `let count`
/// → `const count`. Replaces only the first occurrence; v0 trusts that
/// the variable name is unique at the relevant scope.
fn retype_binding_textual(text: &str, variable: &str, from: &str, to: &str) -> Result<String> {
    let needle = format!("{} {}", from, variable);
    let replacement = format!("{} {}", to, variable);
    let pos = text
        .find(&needle)
        .ok_or_else(|| anyhow!("`{}` not found", needle))?;
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..pos]);
    out.push_str(&replacement);
    out.push_str(&text[pos + needle.len()..]);
    Ok(out)
}

/// Insert a new line whose full contents are `content` immediately after
/// the first line whose text contains `needle`. A trailing newline is
/// appended to `content` if not already present.
fn insert_line_after_matching(text: &str, needle: &str, content: &str) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let idx = lines
        .iter()
        .position(|l| l.contains(needle))
        .ok_or_else(|| anyhow!("no line contains `{}`", needle))?;
    let mut out = String::with_capacity(text.len() + content.len() + 1);
    for (i, l) in lines.iter().enumerate() {
        out.push_str(l);
        if i == idx {
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// Insert `html` on a new line right after the first `<tag>...</tag>`
/// element in `text`, using the indent of the existing element's open tag.
fn insert_html_after_tag(text: &str, tag: &str, html: &str) -> Result<String> {
    let open_pat = format!("<{}", tag);
    let close_pat = format!("</{}>", tag);
    let open_idx = text
        .find(&open_pat)
        .ok_or_else(|| anyhow!("<{}> not found", tag))?;
    let close_rel = text[open_idx..]
        .find(&close_pat)
        .ok_or_else(|| anyhow!("</{}> not found", tag))?;
    let after_close = open_idx + close_rel + close_pat.len();

    let line_start = text[..open_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = text[line_start..open_idx]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();

    let insertion = format!("\n{}{}", indent, html);
    let mut out = String::with_capacity(text.len() + insertion.len());
    out.push_str(&text[..after_close]);
    out.push_str(&insertion);
    out.push_str(&text[after_close..]);
    Ok(out)
}

/// Wrap the children of the first `<tag>...</tag>` element in `wrapper`,
/// where `wrapper` is of the form `<...></...>` (open+close tag with
/// nothing between). The existing children are re-indented by +2 spaces.
fn wrap_element_children(text: &str, tag: &str, wrapper: &str) -> Result<String> {
    let open_pat = format!("<{}>", tag);
    let close_pat = format!("</{}>", tag);
    let open_idx = text
        .find(&open_pat)
        .ok_or_else(|| anyhow!("<{}> not found", tag))?;
    let close_idx_rel = text[open_idx..]
        .find(&close_pat)
        .ok_or_else(|| anyhow!("</{}> not found", tag))?;
    let close_idx = open_idx + close_idx_rel;

    // End of open-tag line (position right after its trailing newline).
    let open_line_end = {
        let nl = text[open_idx..]
            .find('\n')
            .ok_or_else(|| anyhow!("<{}> not followed by newline", tag))?;
        open_idx + nl + 1
    };
    // Start of close-tag line.
    let close_line_start = text[..close_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // Section indent (indent of the open-tag line).
    let open_line_start = text[..open_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let section_indent: String = text[open_line_start..open_idx]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let body_indent = format!("{}  ", section_indent);

    // Split wrapper into open/close.
    let (wrap_open, wrap_rest) = wrapper
        .split_once("></")
        .ok_or_else(|| anyhow!("wrapper must be of form <...></...>"))?;
    let wrap_open_full = format!("{}>", wrap_open);
    let wrap_close_full = format!("</{}", wrap_rest);

    // Re-indent children by +2 spaces on each non-blank line.
    let children_text = &text[open_line_end..close_line_start];
    let reindented: String = children_text
        .split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("  {}", line)
            }
        })
        .collect();

    let mut out = String::with_capacity(text.len() + 64);
    out.push_str(&text[..open_line_end]);
    out.push_str(&format!("{}{}\n", body_indent, wrap_open_full));
    out.push_str(&reindented);
    out.push_str(&format!("{}{}\n", body_indent, wrap_close_full));
    out.push_str(&text[close_line_start..]);
    Ok(out)
}

/// Python `extract_function` for the v0 fixture shape.
/// - Locates `def <fn_name>(`.
/// - Treats every body line at indent `fn_indent + 4` as a new statement.
/// - Collects statements inclusive of body_range.
/// - Emits a new `def new_name(params):` block *before* the source function,
///   containing those statements (same indent level) plus a trailing
///   `return <returns>`.
/// - Replaces the extracted statements with `return new_name(params)` and
///   drops the immediately-following `return <returns>` statement if present.
fn extract_function(
    text: &str,
    fn_name: &str,
    body_range: [usize; 2],
    new_name: &str,
    params: &[String],
    returns: &[String],
) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent_cols = lines[def_idx].len() - lines[def_idx].trim_start().len();
    let body_indent_cols = fn_indent_cols + 4;

    // Segment body into statements.
    let mut stmt_ranges: Vec<(usize, usize)> = Vec::new(); // inclusive line ranges
    let mut i = def_idx + 1;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent_cols = line.len() - line.trim_start().len();
        if !is_blank && indent_cols < body_indent_cols {
            break; // out of function body
        }
        if !is_blank && indent_cols == body_indent_cols {
            // new top-level body statement
            stmt_ranges.push((i, i));
        } else if let Some(last) = stmt_ranges.last_mut() {
            last.1 = i; // continuation or blank line inside current stmt
        } else {
            // blank line before first statement; skip
        }
        i += 1;
    }

    let [lo, hi] = body_range;
    if hi >= stmt_ranges.len() {
        return Err(anyhow!(
            "body_range out of bounds: {} statements present",
            stmt_ranges.len()
        ));
    }
    let extract_start = stmt_ranges[lo].0;
    let extract_end_excl = stmt_ranges[hi].1 + 1;

    // Scan forward from the statement after the extracted range, skipping blank
    // and comment-only statements, and absorb a trailing `return <returns>` if
    // we encounter one before any other real statement. This makes the absorb
    // robust to blank lines / comments between the extracted body and the
    // function's terminal return.
    let mut absorb_return_end = extract_end_excl;
    let want_return = if returns.is_empty() {
        String::new()
    } else {
        format!("return {}", returns.join(", "))
    };
    if !want_return.is_empty() {
        let mut k = hi + 1;
        while k < stmt_ranges.len() {
            let (s, e) = stmt_ranges[k];
            let joined: String = lines[s..=e].concat();
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                // Defensive: stmt_ranges shouldn't contain pure-blank entries,
                // but skip just in case.
                k += 1;
                continue;
            }
            // Comment-only statement: every non-blank line in the range starts with `#`.
            let is_comment_only = lines[s..=e]
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .all(|l| l.starts_with('#'));
            if is_comment_only {
                k += 1;
                continue;
            }
            if trimmed == want_return {
                absorb_return_end = e + 1;
            }
            break;
        }
    }

    // Extracted body text (keeps original indent, which becomes the body
    // indent of the new helper since the helper sits at the same fn_indent).
    let extracted: String = lines[extract_start..extract_end_excl].concat();

    let fn_indent_str: String = " ".repeat(fn_indent_cols);
    let body_indent_str: String = " ".repeat(body_indent_cols);
    let params_joined = params.join(", ");
    let returns_joined = returns.join(", ");

    let mut helper = String::new();
    helper.push_str(&fn_indent_str);
    helper.push_str(&format!("def {}({}):\n", new_name, params_joined));
    helper.push_str(&extracted);
    if !returns.is_empty() {
        helper.push_str(&body_indent_str);
        helper.push_str(&format!("return {}\n", returns_joined));
    }
    helper.push('\n');
    helper.push('\n');

    // Replacement call inside the source function.
    let call_line = if returns.is_empty() {
        format!("{}{}({})\n", body_indent_str, new_name, params_joined)
    } else {
        format!(
            "{}return {}({})\n",
            body_indent_str, new_name, params_joined
        )
    };

    // Reassemble: [lines before def] + helper + [def .. extract_start] + call + [absorb_return_end ..]
    let before_def: String = lines[..def_idx].concat();
    let def_to_extract: String = lines[def_idx..extract_start].concat();
    let after_extract: String = lines[absorb_return_end..].concat();

    let mut out = String::with_capacity(text.len() + 64);
    out.push_str(&before_def);
    out.push_str(&helper);
    out.push_str(&def_to_extract);
    out.push_str(&call_line);
    out.push_str(&after_extract);
    Ok(out)
}

/// Delete the Nth comment (0-indexed) inside the named function's body.
fn delete_nth_comment_in_fn(text: &str, fn_name: &str, n: usize) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent_cols = lines[def_idx].len() - lines[def_idx].trim_start().len();
    let body_indent_cols = fn_indent_cols + 4;

    let mut count = 0usize;
    let mut delete_line: Option<usize> = None;
    let mut i = def_idx + 1;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent_cols = line.len() - line.trim_start().len();
        if !is_blank && indent_cols < body_indent_cols {
            break;
        }
        if !is_blank && indent_cols == body_indent_cols && stripped.trim_start().starts_with('#') {
            if count == n {
                delete_line = Some(i);
                break;
            }
            count += 1;
        }
        i += 1;
    }
    let di = delete_line.ok_or_else(|| anyhow!("comment index {} not found", n))?;
    let mut out = String::with_capacity(text.len());
    for (j, l) in lines.iter().enumerate() {
        if j != di {
            out.push_str(l);
        }
    }
    Ok(out)
}

/// Normalize a comment's text for hashing / equality: trim leading `#`,
/// strip surrounding whitespace. Two comments with identical semantic
/// content but different incidental whitespace hash to the same id.
pub fn normalize_comment_text(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    s.trim().to_string()
}

/// Compute a 12-hex-char content id for a comment node under a parent fn.
/// Identity = (parent_id, node_kind, normalized_text). SHA-256 truncated to
/// 12 hex chars (48 bits) — collision resistance is sufficient within a
/// single worktree where nodes.json acts as the registry and any collision
/// would be caught at record time.
pub fn comment_node_id(parent_fn: &str, raw_text: &str) -> String {
    use sha2::{Digest, Sha256};
    let normalized = normalize_comment_text(raw_text);
    let mut h = Sha256::new();
    h.update(parent_fn.as_bytes());
    h.update(b"|comment|");
    h.update(normalized.as_bytes());
    let digest = h.finalize();
    hex::encode(&digest[..6])
}

/// Delete the first comment in `fn_name`'s body whose normalized text equals
/// `target_text` (also normalized). Returns an error if not found — that's a
/// semantic conflict surface for the merger.
fn delete_comment_by_text_in_fn(text: &str, fn_name: &str, target_text: &str) -> Result<String> {
    let needle = normalize_comment_text(target_text);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent_cols = lines[def_idx].len() - lines[def_idx].trim_start().len();
    let body_indent_cols = fn_indent_cols + 4;

    let mut delete_line: Option<usize> = None;
    let mut i = def_idx + 1;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent_cols = line.len() - line.trim_start().len();
        if !is_blank && indent_cols < body_indent_cols {
            break;
        }
        if !is_blank && indent_cols == body_indent_cols && stripped.trim_start().starts_with('#') {
            if normalize_comment_text(stripped.trim_start()) == needle {
                delete_line = Some(i);
                break;
            }
        }
        i += 1;
    }
    let di = delete_line.ok_or_else(|| {
        anyhow!(
            "delete_node: no comment matching `{}` found in fn `{}`",
            needle,
            fn_name
        )
    })?;
    let mut out = String::with_capacity(text.len());
    for (j, l) in lines.iter().enumerate() {
        if j != di {
            out.push_str(l);
        }
    }
    Ok(out)
}

/// Append a new CSS rule block at the end of the stylesheet.
fn append_css_rule(text: &str, selector: &str, decls: &[Declaration]) -> String {
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(selector);
    out.push_str(" {\n");
    for d in decls {
        out.push_str(&format!("  {}: {};\n", d.prop, d.value));
    }
    out.push_str("}\n");
    out
}

/// Locate the contiguous top-of-file import block and return (start, end) line
/// indices inclusive. Returns None if no imports found.
fn find_import_block(lines: &[&str]) -> Option<(usize, usize)> {
    let mut start = None;
    let mut end = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            if start.is_none() {
                start = Some(i);
            }
            end = Some(i);
        } else if start.is_some() && !trimmed.trim().is_empty() {
            break;
        }
    }
    match (start, end) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
    }
}

/// Sort the imports block alphabetically (by full trimmed line).
fn sort_imports_alphabetic(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some((s, e)) = find_import_block(&lines) else {
        return text.to_string();
    };
    let mut block: Vec<&str> = lines[s..=e].to_vec();
    block.sort_by(|a, b| a.trim().cmp(b.trim()));
    let mut out = String::new();
    out.push_str(&lines[..s].concat());
    out.push_str(&block.concat());
    out.push_str(&lines[e + 1..].concat());
    out
}

/// Insert `import <module>` at the sorted position if the existing block is
/// already sorted, otherwise append at the block's end.
fn add_import_smart(text: &str, module: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let new_line = format!("import {}\n", module);
    let Some((s, e)) = find_import_block(&lines) else {
        let mut out = new_line.clone();
        out.push_str(text);
        return out;
    };
    let block: Vec<&str> = (s..=e).map(|j| lines[j]).collect();
    let mut sorted = block.clone();
    sorted.sort_by(|a, b| a.trim().cmp(b.trim()));
    let is_sorted = block == sorted;

    let insert_at = if is_sorted {
        let mut at = e + 1;
        for j in s..=e {
            if new_line.trim() < lines[j].trim() {
                at = j;
                break;
            }
        }
        at
    } else {
        e + 1
    };

    let mut out = String::new();
    out.push_str(&lines[..insert_at].concat());
    out.push_str(&new_line);
    out.push_str(&lines[insert_at..].concat());
    out
}

/// Replace the first occurrence of `old` within the named function body.
fn replace_in_fn_body(text: &str, fn_name: &str, old: &str, new: &str) -> Result<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let def_prefix = format!("def {}(", fn_name);
    let def_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&def_prefix))
        .ok_or_else(|| anyhow!("def {} not found", fn_name))?;
    let fn_indent_cols = lines[def_idx].len() - lines[def_idx].trim_start().len();
    let body_indent_cols = fn_indent_cols + 4;

    let mut body_end = lines.len();
    let mut i = def_idx + 1;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_end_matches(['\n', '\r']);
        let is_blank = stripped.trim().is_empty();
        let indent_cols = line.len() - line.trim_start().len();
        if !is_blank && indent_cols < body_indent_cols {
            body_end = i;
            break;
        }
        i += 1;
    }

    let before: String = lines[..=def_idx].concat();
    let body: String = lines[def_idx + 1..body_end].concat();
    let after: String = lines[body_end..].concat();

    let pos = body
        .find(old)
        .ok_or_else(|| anyhow!("old substring `{}` not found in body of `{}`", old, fn_name))?;
    let mut rebuilt_body = String::with_capacity(body.len() + new.len());
    rebuilt_body.push_str(&body[..pos]);
    rebuilt_body.push_str(new);
    rebuilt_body.push_str(&body[pos + old.len()..]);

    Ok(before + &rebuilt_body + &after)
}

/// Directories that are never part of the logical source tree. Skipped
/// during `load_tree` to avoid reading binary artifacts, build caches,
/// or VCS internals. Extend this list as more ecosystem conventions
/// surface; a proper `.gitignore`-aware walk (via the `ignore` crate)
/// would be the v2 improvement.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".llm-edit",
    "target",       // Rust / Cargo
    "node_modules", // Node / npm / pnpm
    "__pycache__",  // Python bytecode
    ".venv",        // Python virtualenvs
    "venv",
    ".tox",
    "build", // generic build output
    "dist",
];

/// Walk the CSS AST and collect (start, end) byte ranges of class
/// names inside class_selector nodes that match `old`.
fn collect_css_class_hits(
    node: tree_sitter::Node,
    source: &[u8],
    old: &str,
    hits: &mut Vec<(usize, usize)>,
) {
    // class_selector contains a "." token + a class_name child.
    // We want the class_name child's byte range.
    if node.kind() == "class_name" {
        let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("");
        // Exact match: `.hero` → `.banner`
        if text == old {
            hits.push((node.start_byte(), node.end_byte()));
        }
        // BEM prefix match: `.hero-title` → `.banner-title`
        // (hyphenated variants share a semantic block name)
        if text.starts_with(old) && text.as_bytes().get(old.len()) == Some(&b'-') {
            hits.push((node.start_byte(), node.start_byte() + old.len()));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_css_class_hits(child, source, old, hits);
    }
}

/// Text-based fallback for CSS class rename (used when tree-sitter
/// parse fails or isn't available).
fn rename_css_selector_class_text(text: &str, old: &str, new: &str) -> String {
    let needle = format!(".{}", old);
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(needle_bytes) {
            let after = i + needle_bytes.len();
            let after_ok = after >= bytes.len() || !is_ident_char(bytes[after] as char);
            if after_ok {
                out.push('.');
                out.push_str(new);
                i = after;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

// ----- cross-language propagation registry ---------------------------------
//
// Each entry maps a file extension to a rewrite strategy for a given
// op kind. Adding a new language edge (e.g., "CSS custom property
// rename propagates to HTML inline styles") = adding one match arm.
// This replaces the ad-hoc inline dispatch that was scattered across
// the apply_op match arms.

/// Propagate a symbol rename across all source files in the tree.
/// Dispatches per extension: AST rename for source languages, word-
/// boundary rename for markup (HTML).
fn propagate_symbol_rename(tree: &mut Tree, old: &str, new: &str, respect_shadowing: bool) {
    // Collect paths first to satisfy borrow checker (can't iterate
    // tree.iter_mut() while reading paths).
    let paths: Vec<PathBuf> = tree.keys().cloned().collect();
    for path in paths {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match symbol_rename_strategy(ext) {
            RenameStrategy::Ast => {
                if let Some(text) = tree.get_mut(&path) {
                    *text = rename_in_source_file(text, &path, old, new, respect_shadowing);
                }
            }
            RenameStrategy::WordBoundary => {
                if let Some(text) = tree.get_mut(&path) {
                    *text = rename_word_boundary(text, old, new);
                }
            }
            RenameStrategy::Skip => {}
        }
    }
}

/// Propagate a CSS class rename across CSS and HTML files.
fn propagate_css_class_rename(tree: &mut Tree, old: &str, new: &str) {
    let paths: Vec<PathBuf> = tree.keys().cloned().collect();
    for path in paths {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match css_class_rename_strategy(ext) {
            CssClassStrategy::CssSelector => {
                if let Some(text) = tree.get_mut(&path) {
                    *text = rename_css_selector_class(text, old, new);
                }
            }
            CssClassStrategy::HtmlClassAttr => {
                if let Some(text) = tree.get_mut(&path) {
                    *text = rename_html_class_attr(text, old, new);
                }
            }
            CssClassStrategy::JsxClassName => {
                if let Some(text) = tree.get_mut(&path) {
                    // JSX/TSX className="..." uses the same word-boundary
                    // approach as HTML class attrs.
                    *text = rename_word_boundary(text, old, new);
                }
            }
            CssClassStrategy::Skip => {}
        }
    }
}

/// How to rename a symbol identifier in a file of the given extension.
enum RenameStrategy {
    Ast,
    WordBoundary,
    Skip,
}

fn symbol_rename_strategy(ext: &str) -> RenameStrategy {
    match ext {
        "py" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "rs" => RenameStrategy::Ast,
        "html" | "htm" => RenameStrategy::WordBoundary,
        _ => RenameStrategy::Skip,
    }
}

/// How to rename a CSS class in a file of the given extension.
enum CssClassStrategy {
    CssSelector,
    HtmlClassAttr,
    JsxClassName,
    Skip,
}

fn css_class_rename_strategy(ext: &str) -> CssClassStrategy {
    match ext {
        "css" => CssClassStrategy::CssSelector,
        "html" | "htm" => CssClassStrategy::HtmlClassAttr,
        "jsx" | "tsx" => CssClassStrategy::JsxClassName,
        _ => CssClassStrategy::Skip,
    }
}
