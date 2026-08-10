use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type SymbolId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Type,
    Module,
    Package,
    File,
    ConfigNode,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Type => "type",
            Self::Module => "module",
            Self::Package => "package",
            Self::File => "file",
            Self::ConfigNode => "config_node",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for SymbolKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "function" => Ok(Self::Function),
            "method" => Ok(Self::Method),
            "type" => Ok(Self::Type),
            "module" => Ok(Self::Module),
            "package" => Ok(Self::Package),
            "file" => Ok(Self::File),
            "config_node" => Ok(Self::ConfigNode),
            other => Err(format!("unknown symbol kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LangId {
    Go,
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Java,
    Ruby,
    C,
    Cpp,
    Bash,
    Hcl,
    Yaml,
    Unknown,
}

impl LangId {
    /// True for languages with grammar support — excludes infra/config/lock files.
    pub fn is_source(&self) -> bool {
        !matches!(self, Self::Hcl | Self::Yaml | Self::Unknown)
    }

    pub fn from_path(path: &std::path::Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("go") => Self::Go,
            Some("rs") => Self::Rust,
            Some("py") => Self::Python,
            Some("ts") | Some("tsx") => Self::TypeScript,
            Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Self::JavaScript,
            Some("java") => Self::Java,
            Some("rb") => Self::Ruby,
            Some("c") | Some("h") => Self::C,
            Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") => Self::Cpp,
            Some("sh") | Some("bash") => Self::Bash,
            Some("tf") | Some("hcl") => Self::Hcl,
            Some("yaml") | Some("yml") => Self::Yaml,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for LangId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Java => "java",
            Self::Ruby => "ruby",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Bash => "bash",
            Self::Hcl => "hcl",
            Self::Yaml => "yaml",
            Self::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for LangId {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        match s {
            "go" => Ok(Self::Go),
            "rust" => Ok(Self::Rust),
            "python" => Ok(Self::Python),
            "typescript" => Ok(Self::TypeScript),
            "javascript" => Ok(Self::JavaScript),
            "java" => Ok(Self::Java),
            "ruby" => Ok(Self::Ruby),
            "c" => Ok(Self::C),
            "cpp" => Ok(Self::Cpp),
            "bash" => Ok(Self::Bash),
            "hcl" => Ok(Self::Hcl),
            "yaml" => Ok(Self::Yaml),
            _ => Ok(Self::Unknown),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub kind: SymbolKind,
    pub name: String,
    pub qualified: String,
    pub file: PathBuf,
    pub span: (u32, u32),
    pub lang: LangId,
    pub churn: u32,
    pub age_days: u32,
    pub loc: u32,
    pub complexity: u32,
    pub hotspot: f32,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Calls,
    Contains,
    Imports,
    Cochanges,
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Calls => "calls",
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::Cochanges => "cochanges",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for EdgeKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "calls" => Ok(Self::Calls),
            "contains" => Ok(Self::Contains),
            "imports" => Ok(Self::Imports),
            "cochanges" => Ok(Self::Cochanges),
            other => Err(format!("unknown edge kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub confidence: f32,
    pub meta: Option<EdgeMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    pub path: String,
    pub lang: LangId,
    pub loc: u32,
    pub churn: u32,
    pub age_days: u32,
    pub last_commit_sha: Option<String>,
    pub last_commit_ts: Option<i64>,
    pub hotspot: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoChange {
    pub file_a: String,
    pub file_b: String,
    pub support: u32,
    pub strength: f32,
    pub strength_rev: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFrame {
    pub symbol_id: SymbolId,
    pub name: String,
    pub qualified: String,
    pub file: String,
    pub span: (u32, u32),
    pub mark: DiffMark,
    pub weight: f32,
    pub children: Vec<DiffFrame>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffMark {
    Added,
    Removed,
    Modified,
    Context,
}
