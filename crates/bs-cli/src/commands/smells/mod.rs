mod detectors;

use super::{emit, has_pattern, open_store, Context};
use crate::thresholds::{load_custom_smells, load_thresholds};
use anyhow::Result;
use clap::Args;
use std::collections::HashMap;

use detectors::{
    detect_custom_rules, detect_god_file, detect_semantic, detect_shotgun_surgery,
    detect_stale_core, detect_tangled_pair, generate_recommendations,
};

#[derive(Args)]
pub struct SmellsArgs {
    /// Emit tool recommendations for security-sensitive co-change pairs
    #[arg(long)]
    pub recommend: bool,
}

#[derive(Default, serde::Serialize)]
pub(super) struct SmellReport {
    pub shotgun_surgery: Vec<ShotgunEntry>,
    pub god_file: Vec<String>,
    pub stale_core: Vec<String>,
    pub tangled_pair: Vec<(String, String)>,
    pub semantic: Vec<SemanticSmell>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(serde::Serialize)]
pub(super) struct ShotgunEntry {
    pub file: String,
    pub partners: usize,
}

#[derive(serde::Serialize)]
pub(super) struct SemanticSmell {
    pub kind: String,
    pub symbol: String,
    pub file: String,
    pub detail: String,
}

#[derive(serde::Serialize)]
pub(super) struct Recommendation {
    pub tool: String,
    pub reason: String,
    pub files: Vec<String>,
}

pub fn run(ctx: &Context, args: &SmellsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_all_file_stats()?;
    let cochange = store.get_all_cochange(5)?;

    let thresholds = load_thresholds(&ctx.repo_root);
    let custom_rules = load_custom_smells(&ctx.repo_root);

    let mut report = SmellReport::default();
    detect_shotgun_surgery(&cochange, 0.5, 4, &mut report);
    detect_god_file(&stats, &mut report);
    detect_stale_core(&stats, &mut report);
    detect_tangled_pair(&cochange, 0.8, &mut report);

    let symbols = store.all_symbols_with_patterns().unwrap_or_default();
    let edge_counts = store.get_call_edge_counts().unwrap_or_default();
    detect_semantic(&symbols, &edge_counts, &thresholds, &mut report);
    detect_custom_rules(&symbols, &custom_rules, &mut report);

    if args.recommend {
        generate_recommendations(&cochange, &mut report);
    }

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&report).unwrap_or_default(),
        bs_render::OutputFormat::Mermaid => {
            let findings = smells_to_findings(&report);
            bs_render::mermaid::render_class(&findings, ctx.no_fence)
        }
        bs_render::OutputFormat::Dot => {
            let findings = smells_to_findings(&report);
            bs_render::dot::render_class(&findings, ctx.no_fence)
        }
        _ => format_report(&report, args.recommend),
    };

    emit(ctx, &out);
    Ok(())
}

fn semantic_kind_desc(kind: &str) -> &'static str {
    match kind {
        "lock_across_await" => "mutex held across .await — deadlock risk",
        "sync_in_async" => "blocking call inside async fn — starves executor",
        "alloc_in_hotspot" => "heavy allocations in high-churn hot symbol",
        "structural_violation" => "exceeds absolute complexity or LOC limit — split required",
        "high_complexity_bottleneck" => "complex + central + hot — hardest to change safely",
        "spawn_in_tight_loop" => "spawning threads/tasks inside a loop — explosion risk",
        "unbalanced_fanout" => "many callees, almost no callers — possibly dead code",
        _ => "structural anomaly",
    }
}

fn smells_to_findings(report: &SmellReport) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for s in &report.semantic {
        out.push((s.file.clone(), s.kind.clone()));
    }
    for f in &report.god_file {
        out.push((f.clone(), "god_file".to_string()));
    }
    for f in &report.stale_core {
        out.push((f.clone(), "stale_core".to_string()));
    }
    for e in &report.shotgun_surgery {
        out.push((e.file.clone(), "shotgun_surgery".to_string()));
    }
    for (a, b) in &report.tangled_pair {
        out.push((a.clone(), "tangled_pair".to_string()));
        out.push((b.clone(), "tangled_pair".to_string()));
    }
    out
}

fn format_report(report: &SmellReport, recommend: bool) -> String {
    let mut out = String::new();

    out.push_str("=== Antipattern Report ===\n\n");

    out.push_str(&format!(
        "shotgun-surgery ({} files with ≥4 strong co-change partners):\n",
        report.shotgun_surgery.len()
    ));
    if report.shotgun_surgery.is_empty() {
        out.push_str("  (none)\n");
    }
    for e in &report.shotgun_surgery {
        out.push_str(&format!("  {} ({} partners)\n", e.file, e.partners));
    }

    out.push('\n');
    out.push_str(&format!("god-file ({} files):\n", report.god_file.len()));
    if report.god_file.is_empty() {
        out.push_str("  (none)\n");
    }
    for f in &report.god_file {
        out.push_str(&format!("  {}\n", f));
    }

    out.push('\n');
    out.push_str(&format!("stale-core ({} files):\n", report.stale_core.len()));
    if report.stale_core.is_empty() {
        out.push_str("  (none)\n");
    }
    for f in &report.stale_core {
        out.push_str(&format!("  {}\n", f));
    }

    out.push('\n');
    out.push_str(&format!(
        "tangled-pair ({} pairs with strength≥0.8 both ways):\n",
        report.tangled_pair.len()
    ));
    if report.tangled_pair.is_empty() {
        out.push_str("  (none)\n");
    }
    for (a, b) in &report.tangled_pair {
        out.push_str(&format!("  {} ↔ {}\n", a, b));
    }

    out.push('\n');
    out.push_str(&format!("semantic ({} findings):\n", report.semantic.len()));
    if report.semantic.is_empty() {
        out.push_str("  (none)\n");
    } else {
        let mut by_kind: HashMap<String, Vec<&SemanticSmell>> = HashMap::new();
        for s in &report.semantic {
            by_kind.entry(s.kind.clone()).or_default().push(s);
        }
        let mut kinds: Vec<_> = by_kind.keys().cloned().collect();
        kinds.sort();
        for kind in &kinds {
            let entries = &by_kind[kind];
            let desc = semantic_kind_desc(kind);
            out.push_str(&format!(
                "  [{}] — {} ({} symbols)\n",
                kind,
                desc,
                entries.len()
            ));
            for s in entries.iter().take(3) {
                out.push_str(&format!("    • {}\n", s.symbol));
            }
            if entries.len() > 3 {
                out.push_str(&format!("    … and {} more\n", entries.len() - 3));
            }
        }
    }

    if recommend {
        out.push('\n');
        out.push_str(&format!(
            "recommendations ({}):\n",
            report.recommendations.len()
        ));
        if report.recommendations.is_empty() {
            out.push_str("  (none)\n");
        }
        for r in &report.recommendations {
            out.push_str(&format!(
                "  $ {}\n  reason: {}\n  files:\n",
                r.tool, r.reason
            ));
            for f in &r.files {
                out.push_str(&format!("    {}\n", f));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::detectors::{detect_custom_rules, detect_god_file, detect_semantic,
        detect_shotgun_surgery, detect_stale_core};
    use bs_core::{CoChange, FileStat, Symbol, SymbolKind};
    use crate::thresholds::{CustomSmellRule, Thresholds};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn file_stat(path: &str, loc: u32, churn: u32, age_days: u32, hotspot: f32) -> FileStat {
        FileStat {
            path: path.to_string(),
            lang: bs_core::LangId::Rust,
            loc,
            churn,
            age_days,
            last_commit_sha: None,
            last_commit_ts: None,
            hotspot,
        }
    }

    fn sym(name: &str, patterns: &[&str], hotspot: f32, complexity: u32) -> Symbol {
        Symbol {
            id: name.to_string(),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qualified: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            span: (1, 10),
            lang: bs_core::LangId::Rust,
            churn: 2,
            age_days: 10,
            loc: 10,
            complexity,
            hotspot,
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn cochange(a: &str, b: &str, strength: f32) -> CoChange {
        CoChange {
            file_a: a.to_string(),
            file_b: b.to_string(),
            support: 5,
            strength,
            strength_rev: strength,
        }
    }

    #[test]
    fn test_detect_god_file_p95() {
        let mut stats: Vec<FileStat> = (0..20)
            .map(|i| file_stat(&format!("f{}.rs", i), i * 10 + 1, i + 1, 10, 0.0))
            .collect();
        stats.push(file_stat("god.rs", 5000, 500, 5, 0.9));
        let mut report = SmellReport::default();
        detect_god_file(&stats, &mut report);
        assert!(report.god_file.contains(&"god.rs".to_string()), "god.rs must be flagged");
    }

    #[test]
    fn test_detect_shotgun_surgery() {
        let pairs: Vec<CoChange> = (0..5)
            .map(|i| cochange("hub.rs", &format!("spoke{}.rs", i), 0.8))
            .collect();
        let mut report = SmellReport::default();
        detect_shotgun_surgery(&pairs, 0.5, 4, &mut report);
        assert!(
            report.shotgun_surgery.iter().any(|e| e.file == "hub.rs" && e.partners >= 4),
            "hub.rs must be flagged for shotgun surgery"
        );
    }

    #[test]
    fn test_detect_stale_core() {
        let stats = vec![
            file_stat("old_core.rs", 100, 50, 800, 0.5),
            file_stat("new_file.rs", 100, 50, 30, 0.5),
        ];
        let mut report = SmellReport::default();
        detect_stale_core(&stats, &mut report);
        assert!(report.stale_core.contains(&"old_core.rs".to_string()), "old_core.rs must be stale-core");
        assert!(!report.stale_core.contains(&"new_file.rs".to_string()), "new_file.rs must not be stale-core");
    }

    #[test]
    fn test_detect_semantic_lock_await() {
        let s = sym("risky_fn", &["lock", "await"], 0.3, 3);
        let mut report = SmellReport::default();
        detect_semantic(&[s], &HashMap::new(), &Thresholds::default(), &mut report);
        assert!(report.semantic.iter().any(|e| e.kind == "lock_across_await"),
            "lock+await must produce lock_across_await smell");
    }

    #[test]
    fn test_detect_semantic_spawn_loop() {
        let s = sym("leaky_fn", &["spawn", "loop"], 0.3, 3);
        let mut report = SmellReport::default();
        detect_semantic(&[s], &HashMap::new(), &Thresholds::default(), &mut report);
        assert!(report.semantic.iter().any(|e| e.kind == "spawn_in_tight_loop"),
            "spawn+loop must produce spawn_in_tight_loop");
    }

    #[test]
    fn test_detect_semantic_no_patterns_skipped() {
        let s = sym("clean_fn", &[], 0.9, 20);
        let mut report = SmellReport::default();
        detect_semantic(&[s], &HashMap::new(), &Thresholds::default(), &mut report);
        assert!(report.semantic.is_empty(), "no patterns = no semantic smells");
    }

    #[test]
    fn test_detect_custom_rules_fires_when_all_patterns_match() {
        let s = sym("danger_fn", &["lock", "await", "spawn"], 0.5, 5);
        let rule = CustomSmellRule {
            name: "deadlock_spawn".to_string(),
            description: "holds lock while spawning across await".to_string(),
            patterns: vec!["lock".to_string(), "spawn".to_string()],
            severity: "high".to_string(),
        };
        let mut report = SmellReport::default();
        detect_custom_rules(&[s], &[rule], &mut report);
        assert!(report.semantic.iter().any(|e| e.kind == "deadlock_spawn"),
            "custom rule must fire when all patterns present");
    }

    #[test]
    fn test_detect_custom_rules_no_fire_on_partial_match() {
        let s = sym("safe_fn", &["lock"], 0.5, 5);
        let rule = CustomSmellRule {
            name: "deadlock_spawn".to_string(),
            description: "holds lock while spawning".to_string(),
            patterns: vec!["lock".to_string(), "spawn".to_string()],
            severity: "high".to_string(),
        };
        let mut report = SmellReport::default();
        detect_custom_rules(&[s], &[rule], &mut report);
        assert!(report.semantic.is_empty(),
            "custom rule must not fire when only partial patterns present");
    }
}
