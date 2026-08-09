use crate::TreeNode;
use bs_core::{CoChange, Symbol};
use serde::Serialize;
use serde_json::Value;

const SCHEMA_VERSION: u32 = 1;
const BORESCOPE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
pub struct JsonOutput {
    pub borescope: &'static str,
    pub schema: u32,
    pub query: QueryMeta,
    pub root: Option<TreeNode>,
    pub cochange: Vec<CochangeEntry>,
    pub truncated: TruncatedInfo,
    pub unresolved: Vec<Value>,
}

#[derive(Serialize)]
pub struct QueryMeta {
    pub cmd: String,
    pub target: String,
    pub depth: u32,
    pub weight: String,
}

#[derive(Serialize)]
pub struct CochangeEntry {
    pub file: String,
    pub strength: f32,
    pub support: u32,
}

#[derive(Serialize)]
pub struct TruncatedInfo {
    pub depth: bool,
    pub nodes_omitted: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn render_json(
    cmd: &str,
    target: &str,
    depth: u32,
    weight_name: &str,
    root: Option<TreeNode>,
    cochange: &[CoChange],
    truncated_depth: bool,
    nodes_omitted: usize,
) -> String {
    let output = JsonOutput {
        borescope: BORESCOPE_VERSION,
        schema: SCHEMA_VERSION,
        query: QueryMeta {
            cmd: cmd.to_string(),
            target: target.to_string(),
            depth,
            weight: weight_name.to_string(),
        },
        root,
        cochange: cochange
            .iter()
            .map(|c| CochangeEntry {
                file: if c.file_a == target {
                    c.file_b.clone()
                } else {
                    c.file_a.clone()
                },
                strength: c.strength.max(c.strength_rev),
                support: c.support,
            })
            .collect(),
        truncated: TruncatedInfo {
            depth: truncated_depth,
            nodes_omitted,
        },
        unresolved: vec![],
    };
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn render_symbol_json(sym: &Symbol, cochange: &[CoChange]) -> String {
    #[derive(Serialize)]
    struct SymbolJson<'a> {
        borescope: &'static str,
        schema: u32,
        symbol: &'a Symbol,
        cochange: Vec<CochangeEntry>,
    }
    let out = SymbolJson {
        borescope: BORESCOPE_VERSION,
        schema: SCHEMA_VERSION,
        symbol: sym,
        cochange: cochange
            .iter()
            .map(|c| CochangeEntry {
                file: c.file_b.clone(),
                strength: c.strength,
                support: c.support,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&out).unwrap_or_default()
}
