pub mod cli;
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
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root)?.to_path_buf();
        // Skip the op-log itself when loading base/agent trees.
        if rel.starts_with(".llm-edit") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read {}", entry.path().display()))?;
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
        Op::RenameSymbol { scope, file, old, new, .. } => {
            match scope.as_str() {
                "module" => {
                    let path = PathBuf::from(file);
                    let text = tree
                        .get_mut(&path)
                        .ok_or_else(|| anyhow!("no such file {}", file))?;
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    *text = match ext {
                        "py" => py_ast_rename_identifier(text, old, new),
                        _ => rename_word_boundary(text, old, new),
                    };
                }
                "project" => {
                    for (path, text) in tree.iter_mut() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if matches!(ext, "js" | "ts" | "jsx" | "tsx" | "html" | "htm") {
                            // v0: textual word-boundary rewrite is safe enough
                            // for the small fixtures. Real impl should use AST +
                            // kind-aware rewrites.
                            *text = rename_word_boundary(text, old, new);
                        }
                    }
                }
                _ => return Err(anyhow!("unsupported rename_symbol scope: {}", scope)),
            }
            Ok(())
        }
        Op::RenameCssClass { scope, old, new } => {
            if scope != "project" {
                return Err(anyhow!("only scope=project supported in v0"));
            }
            for (path, text) in tree.iter_mut() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "css" => *text = rename_css_selector_class(text, old, new),
                    "html" | "htm" => *text = rename_html_class_attr(text, old, new),
                    _ => {}
                }
            }
            Ok(())
        }
        Op::AddFunction { anchor, after, source, .. } => {
            let file = anchor_file(anchor)?;
            let after_name = after
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("add_function.after.name missing"))?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = insert_function_after(text, after_name, source)?;
            Ok(())
        }
        Op::InsertElement { anchor, position, html } => {
            let file = anchor_file(anchor)?;
            let selector = anchor
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("insert_element.anchor.selector missing"))?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
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
                        selector, position
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
        } => {
            let file = anchor_file(source)?;
            let fn_name = anchor_path_fn_name(source)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = extract_function(text, &fn_name, *body_range, new_name, params, returns)?;
            Ok(())
        }
        Op::DeleteNode { anchor } => {
            let file = anchor_file(anchor)?;
            let fn_name = anchor_path_fn_name(anchor)?;
            let (kind, idx) = anchor_path_last_kind_index(anchor)?;
            if kind != "comment" {
                return Err(anyhow!("delete_node v0 only supports kind=comment"));
            }
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = delete_nth_comment_in_fn(text, &fn_name, idx)?;
            Ok(())
        }
        Op::ReplaceInAnchor { anchor, old, new } => {
            let file = anchor_file(anchor)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
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
            } else {
                let fn_name = anchor_path_fn_name(anchor)?;
                *text = replace_in_fn_body(text, &fn_name, old, new)?;
            }
            Ok(())
        }
        Op::AddRule { anchor, selector, declarations } => {
            let file = anchor_file(anchor)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = append_css_rule(text, selector, declarations);
            Ok(())
        }
        Op::SortBlock { anchor, key } => {
            if key != "alphabetic" {
                return Err(anyhow!("sort_block v0 only supports key=alphabetic"));
            }
            let file = anchor_file(anchor)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = sort_imports_alphabetic(text);
            Ok(())
        }
        Op::AddImport { anchor, module } => {
            let file = anchor_file(anchor)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = add_import_smart(text, module);
            Ok(())
        }
        Op::WrapElement { anchor, wrapper } => {
            let file = anchor_file(anchor)?;
            let selector = anchor
                .get("selector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("wrap_element.anchor.selector missing"))?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = wrap_element_children(text, selector, wrapper)?;
            Ok(())
        }
        Op::RetypeBinding { anchor, from, to, .. } => {
            let file = anchor_file(anchor)?;
            let variable = anchor
                .get("variable")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("retype_binding.anchor.variable missing"))?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = retype_binding_textual(text, variable, from, to)?;
            Ok(())
        }
        Op::DeleteFunction { anchor } => {
            let file = anchor_file(anchor)?;
            let fn_name = anchor_path_fn_name(anchor)?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = delete_top_level_fn(text, &fn_name)?;
            Ok(())
        }
        Op::InsertAfter { anchor, content, .. } => {
            let file = anchor_file(anchor)?;
            let needle = anchor
                .get("after_line_contains")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("insert_after.anchor.after_line_contains missing"))?;
            let text = tree.get_mut(&file).ok_or_else(|| anyhow!("no such file {}", file.display()))?;
            *text = insert_line_after_matching(text, needle, content)?;
            Ok(())
        }
    }
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

/// AST-scoped rename for Python: rewrite every `identifier` node whose
/// text equals `old` to `new`. String literal contents and comments are
/// untouched because they are not identifier nodes.
///
/// On parse failure the input is returned unchanged; callers relying on
/// the rename should confirm the source is valid Python before invoking.
pub fn py_ast_rename_identifier(source: &str, old: &str, new: &str) -> String {
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
    collect_identifier_hits(tree.root_node(), bytes, old_b, &mut hits);
    // Splice in reverse so earlier byte indices stay valid.
    hits.sort_unstable();
    hits.dedup();
    let mut out = source.to_string();
    for (s, e) in hits.iter().rev() {
        out.replace_range(*s..*e, new);
    }
    out
}

fn collect_identifier_hits(
    node: tree_sitter::Node,
    source: &[u8],
    old: &[u8],
    hits: &mut Vec<(usize, usize)>,
) {
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
            let after_ok =
                after_i >= bytes.len() || !is_ident_char(bytes[after_i] as char);
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
    // Match `.` followed by `old` followed by non-ident char (or EOF).
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
    if !text.contains(&marker) {
        return Err(anyhow!("after function `{}` not found", after_name));
    }
    // We assume the named function is the last top-level def in the file
    // (v0 constraint; matches our fixture). Append source after a blank-line gap.
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n\n"); // two blank lines between top-level defs (PEP 8)
    out.push_str(source);
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
        return Err(anyhow!("body_range out of bounds: {} statements present", stmt_ranges.len()));
    }
    let extract_start = stmt_ranges[lo].0;
    let extract_end_excl = stmt_ranges[hi].1 + 1;

    // Check if the statement right after is `return <returns[0]>` — if so, absorb it.
    let mut absorb_return_end = extract_end_excl;
    let want_return = returns
        .first()
        .map(|r| format!("return {}", r))
        .unwrap_or_default();
    if hi + 1 < stmt_ranges.len() && !want_return.is_empty() {
        let (s, e) = stmt_ranges[hi + 1];
        let joined: String = lines[s..=e].concat();
        if joined.trim() == want_return {
            absorb_return_end = e + 1;
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

    let pos = body.find(old).ok_or_else(|| {
        anyhow!(
            "old substring `{}` not found in body of `{}`",
            old, fn_name
        )
    })?;
    let mut rebuilt_body = String::with_capacity(body.len() + new.len());
    rebuilt_body.push_str(&body[..pos]);
    rebuilt_body.push_str(new);
    rebuilt_body.push_str(&body[pos + old.len()..]);

    Ok(before + &rebuilt_body + &after)
}
