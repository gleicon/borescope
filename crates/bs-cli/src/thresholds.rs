use serde::Deserialize;
use std::path::Path;

/// Risk thresholds, configurable via `.borescope/thresholds.toml`.
/// All values fall back to the hardcoded defaults when the file is absent.
#[derive(Debug, Clone)]
pub struct Thresholds {
    /// Used in the compound `high_complexity_bottleneck` smell (needs hotspot + fanin too).
    pub hotspot_high: f32,
    pub hotspot_medium: f32,
    pub complexity_high: u32,
    pub fanin_high: u32,
    /// Hard structural limit — `structural_violation` fires when complexity exceeds this
    /// regardless of hotspot or fanin. Independent of the compound bottleneck check.
    pub complexity_absolute: u32,
    /// Hard structural limit — `structural_violation` fires when function LOC exceeds this.
    pub loc_high: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            hotspot_high: 0.7,
            hotspot_medium: 0.5,
            complexity_high: 10,
            fanin_high: 8,
            complexity_absolute: 22,
            loc_high: 200,
        }
    }
}

/// A user-defined smell rule from `.borescope/smells.toml`.
/// A rule fires when ALL listed patterns appear on the same symbol.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomSmellRule {
    pub name: String,
    pub description: String,
    pub patterns: Vec<String>,
    pub severity: String,
}

#[derive(Deserialize, Default)]
struct ThresholdToml {
    default: Option<DefaultSection>,
}

#[derive(Deserialize)]
struct DefaultSection {
    hotspot_high: Option<f32>,
    hotspot_medium: Option<f32>,
    complexity_high: Option<u32>,
    fanin_high: Option<u32>,
    complexity_absolute: Option<u32>,
    loc_high: Option<u32>,
}

#[derive(Deserialize, Default)]
struct SmellsToml {
    rules: Option<Vec<CustomSmellRule>>,
}

/// Load risk thresholds from `.borescope/thresholds.toml`, falling back to defaults.
pub fn load_thresholds(repo_root: &Path) -> Thresholds {
    let path = repo_root.join(".borescope").join("thresholds.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Thresholds::default();
    };
    let Ok(parsed) = toml::from_str::<ThresholdToml>(&content) else {
        return Thresholds::default();
    };
    let mut t = Thresholds::default();
    if let Some(d) = parsed.default {
        if let Some(v) = d.hotspot_high {
            t.hotspot_high = v;
        }
        if let Some(v) = d.hotspot_medium {
            t.hotspot_medium = v;
        }
        if let Some(v) = d.complexity_high {
            t.complexity_high = v;
        }
        if let Some(v) = d.fanin_high {
            t.fanin_high = v;
        }
        if let Some(v) = d.complexity_absolute {
            t.complexity_absolute = v;
        }
        if let Some(v) = d.loc_high {
            t.loc_high = v;
        }
    }
    t
}

/// Load custom smell rules from `.borescope/smells.toml`. Returns empty vec if absent.
pub fn load_custom_smells(repo_root: &Path) -> Vec<CustomSmellRule> {
    let path = repo_root.join(".borescope").join("smells.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let Ok(parsed) = toml::from_str::<SmellsToml>(&content) else {
        return vec![];
    };
    parsed.rules.unwrap_or_default()
}
