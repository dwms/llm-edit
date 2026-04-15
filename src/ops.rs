use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Declaration {
    pub prop: String,
    pub value: String,
}

fn default_on_violation() -> String {
    "conflict".to_string()
}

/// An anchor is stored untyped and validated per op where used.
/// This keeps the v0 shape flexible while we learn what the anchor
/// model really needs.
pub type Anchor = Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    RenameSymbol {
        scope: String,
        #[serde(default)]
        file: String,
        old: String,
        new: String,
        #[serde(default)]
        kinds: Vec<String>,
    },
    RenameCssClass {
        scope: String,
        old: String,
        new: String,
    },
    AddFunction {
        anchor: Anchor,
        after: Value,
        #[allow(dead_code)]
        name: String,
        source: String,
    },
    InsertElement {
        anchor: Anchor,
        position: String,
        html: String,
    },
    ExtractFunction {
        source: Anchor,
        body_range: [usize; 2],
        new_name: String,
        #[serde(default)]
        params: Vec<String>,
        #[serde(default)]
        returns: Vec<String>,
    },
    DeleteNode {
        anchor: Anchor,
    },
    ReplaceInAnchor {
        anchor: Anchor,
        old: String,
        new: String,
    },
    AddRule {
        anchor: Anchor,
        selector: String,
        declarations: Vec<Declaration>,
    },
    SortBlock {
        anchor: Anchor,
        key: String,
    },
    AddImport {
        anchor: Anchor,
        module: String,
    },
    WrapElement {
        anchor: Anchor,
        wrapper: String,
    },
    RetypeBinding {
        anchor: Anchor,
        from: String,
        to: String,
        #[serde(default)]
        precondition: Value,
        /// What to do if the precondition is invalidated by the other
        /// agent's ops. `"conflict"` (default) surfaces a semantic
        /// conflict. `"drop"` silently discards this retype.
        #[serde(default = "default_on_violation")]
        on_violation: String,
    },
    InsertAfter {
        anchor: Anchor,
        content: String,
        #[serde(default)]
        reassigns: Vec<String>,
    },
    DeleteFunction {
        anchor: Anchor,
    },
}

pub fn parse_log(text: &str) -> anyhow::Result<Vec<Op>> {
    let mut ops = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let op: Op = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("parse log line {}: {}", i + 1, e))?;
        ops.push(op);
    }
    Ok(ops)
}
