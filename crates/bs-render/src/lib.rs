pub mod dot;
pub mod folded;
pub mod html;
pub mod json;
pub mod mermaid;
pub mod tree;
pub mod tui;

use bs_core::{FileStat, Symbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Tree,
    Folded,
    Json,
    Html,
    Tui,
    /// Mermaid diagram (fenced ```mermaid``` block by default; use --no-fence for raw syntax).
    Mermaid,
    /// Graphviz DOT diagram (fenced ```dot``` block by default; use --no-fence for raw syntax).
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    None,
    Loc,
    Fanin,
    Churn,
    Hotspot,
    Diff,
}

impl Weight {
    pub fn describe(&self) -> &'static str {
        match self {
            Weight::None => "score: none (all 0.0 — use --weight to enable)",
            Weight::Loc => "score: loc  (lines of code, normalized 0–1)",
            Weight::Fanin => "score: fanin  (how many callers, normalized 0–1)",
            Weight::Churn => "score: churn  (git commit frequency, normalized 0–1)",
            Weight::Hotspot => "score: hotspot  (churn × recency, 0=cold 1=hot)",
            Weight::Diff => "score: diff  (mark: + added  - removed  ~ changed)",
        }
    }

    pub fn score_file(&self, stat: &FileStat, max_churn: f32, max_loc: f32) -> f32 {
        match self {
            Weight::None => 0.0,
            Weight::Loc => stat.loc as f32 / max_loc.max(1.0),
            Weight::Churn => stat.churn as f32 / max_churn.max(1.0),
            Weight::Hotspot => stat.hotspot,
            _ => 0.0,
        }
    }

    /// Return a normalized 0.0–1.0 weight for `sym`. `Weight::Fanin` and `Weight::Diff`
    /// always return 0.0 here — callers that need fanin scores must pre-compute them from
    /// `get_call_edge_counts` and set `TreeNode::weight` directly.
    pub fn score_symbol(&self, sym: &Symbol, max_churn: f32, max_loc: f32) -> f32 {
        match self {
            Weight::None => 0.0,
            Weight::Loc => sym.loc as f32 / max_loc.max(1.0),
            Weight::Churn => sym.churn as f32 / max_churn.max(1.0),
            Weight::Hotspot => sym.hotspot,
            // Fanin requires edge counts passed in separately; caller pre-computes weight
            Weight::Fanin | Weight::Diff => 0.0,
        }
    }

    /// Returns true if this weight mode requires a revision pair (diff/branch commands only).
    pub fn requires_diff_context(&self) -> bool {
        matches!(self, Weight::Diff)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub name: String,
    pub qualified: String,
    pub file: String,
    pub span: (u32, u32),
    pub weight: f32,
    pub confidence: f32,
    pub mark: Option<String>, // "+", "-", "~", or None
    /// True when the callee could not be resolved to any symbol in the indexed repo.
    /// These are leaf nodes — external library calls, stdlib, or unindexed code.
    #[serde(default)]
    pub external: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn leaf(
        id: String,
        name: String,
        qualified: String,
        file: String,
        span: (u32, u32),
    ) -> Self {
        Self {
            id,
            name,
            qualified,
            file,
            span,
            weight: 0.0,
            confidence: 1.0,
            mark: None,
            external: false,
            children: vec![],
        }
    }
}
