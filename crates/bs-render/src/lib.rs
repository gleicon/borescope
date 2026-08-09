pub mod folded;
pub mod html;
pub mod json;
pub mod tree;

use bs_core::{FileStat, Symbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Tree,
    Folded,
    Json,
    Html,
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
    pub fn score_file(&self, stat: &FileStat, max_churn: f32, max_loc: f32) -> f32 {
        match self {
            Weight::None => 0.0,
            Weight::Loc => stat.loc as f32 / max_loc.max(1.0),
            Weight::Churn => stat.churn as f32 / max_churn.max(1.0),
            Weight::Hotspot => stat.hotspot,
            _ => 0.0,
        }
    }

    pub fn score_symbol(&self, sym: &Symbol, max_churn: f32, max_loc: f32) -> f32 {
        match self {
            Weight::None => 0.0,
            Weight::Loc => sym.loc as f32 / max_loc.max(1.0),
            Weight::Churn => sym.churn as f32 / max_churn.max(1.0),
            Weight::Hotspot => sym.hotspot,
            _ => 0.0,
        }
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
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn leaf(id: String, name: String, qualified: String, file: String, span: (u32, u32)) -> Self {
        Self {
            id,
            name,
            qualified,
            file,
            span,
            weight: 0.0,
            confidence: 1.0,
            mark: None,
            children: vec![],
        }
    }
}
