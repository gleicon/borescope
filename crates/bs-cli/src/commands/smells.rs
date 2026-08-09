use super::{emit, has_pattern, open_store, Context};
use anyhow::Result;
use bs_core::FileStat;
use clap::Args;
use std::cmp::Reverse;
use std::collections::HashMap;

#[derive(Args)]
pub struct SmellsArgs {
    /// Emit tool recommendations for security-sensitive co-change pairs
    #[arg(long)]
    pub recommend: bool,
}

pub fn run(ctx: &Context, args: &SmellsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_all_file_stats()?;
    let cochange = store.get_all_cochange(5)?;

    let mut report = SmellReport::default();
    detect_shotgun_surgery(&cochange, 0.5, 4, &mut report);
    detect_god_file(&stats, &mut report);
    detect_stale_core(&stats, &mut report);
    detect_tangled_pair(&cochange, 0.8, &mut report);

    // Semantic pattern detectors — need symbol patterns + call edge counts
    let symbols = store.all_symbols_with_patterns().unwrap_or_default();
    let edge_counts = store.get_call_edge_counts().unwrap_or_default();
    detect_semantic(&symbols, &edge_counts, &mut report);

    if args.recommend {
        generate_recommendations(&cochange, &mut report);
    }

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&report).unwrap_or_default(),
        _ => format_report(&report, args.recommend),
    };

    emit(ctx, &out);
    Ok(())
}

#[derive(Default, serde::Serialize)]
struct SmellReport {
    shotgun_surgery: Vec<ShotgunEntry>,
    god_file: Vec<String>,
    stale_core: Vec<String>,
    tangled_pair: Vec<(String, String)>,
    semantic: Vec<SemanticSmell>,
    recommendations: Vec<Recommendation>,
}

#[derive(serde::Serialize)]
struct ShotgunEntry {
    file: String,
    partners: usize,
}

#[derive(serde::Serialize)]
struct SemanticSmell {
    kind: String,
    symbol: String,
    file: String,
    detail: String,
}

#[derive(serde::Serialize)]
struct Recommendation {
    tool: String,
    reason: String,
    files: Vec<String>,
}

fn detect_shotgun_surgery(
    cochange: &[bs_core::CoChange],
    min_strength: f32,
    min_partners: usize,
    report: &mut SmellReport,
) {
    let mut partner_count: HashMap<String, usize> = HashMap::new();
    for c in cochange {
        if c.strength >= min_strength || c.strength_rev >= min_strength {
            *partner_count.entry(c.file_a.clone()).or_default() += 1;
            *partner_count.entry(c.file_b.clone()).or_default() += 1;
        }
    }
    for (file, count) in partner_count {
        if count >= min_partners {
            report.shotgun_surgery.push(ShotgunEntry {
                file,
                partners: count,
            });
        }
    }
    report.shotgun_surgery.sort_by_key(|e| Reverse(e.partners));
}

fn detect_god_file(stats: &[FileStat], report: &mut SmellReport) {
    if stats.is_empty() {
        return;
    }
    let p95_loc = percentile(stats.iter().map(|s| s.loc as f64).collect(), 95.0);
    let p95_churn = percentile(stats.iter().map(|s| s.churn as f64).collect(), 95.0);

    for s in stats {
        if s.loc as f64 >= p95_loc && s.churn as f64 >= p95_churn {
            report.god_file.push(s.path.clone());
        }
    }
}

fn detect_stale_core(stats: &[FileStat], report: &mut SmellReport) {
    if stats.is_empty() {
        return;
    }
    let p90_churn = percentile(stats.iter().map(|s| s.churn as f64).collect(), 90.0);
    const TWO_YEARS_DAYS: u32 = 730;

    for s in stats {
        if s.age_days >= TWO_YEARS_DAYS && s.churn as f64 >= p90_churn {
            report.stale_core.push(s.path.clone());
        }
    }
}

fn detect_tangled_pair(
    cochange: &[bs_core::CoChange],
    min_strength: f32,
    report: &mut SmellReport,
) {
    for c in cochange {
        if c.strength >= min_strength && c.strength_rev >= min_strength {
            report
                .tangled_pair
                .push((c.file_a.clone(), c.file_b.clone()));
        }
    }
}

fn detect_semantic(
    symbols: &[bs_core::Symbol],
    edge_counts: &HashMap<String, (u32, u32)>,
    report: &mut SmellReport,
) {
    for sym in symbols {
        let pats = &sym.patterns;
        if pats.is_empty() {
            continue;
        }

        let has_lock = has_pattern(pats, "lock");
        let has_await = has_pattern(pats, "await");
        let has_block_on = has_pattern(pats, "block_on");
        let has_spawn = has_pattern(pats, "spawn");
        let has_loop = has_pattern(pats, "loop");
        let alloc_count = pats.iter().filter(|p| p.as_str() == "alloc").count();

        let (fanin, fanout) = edge_counts
            .get(&sym.id.to_string())
            .copied()
            .unwrap_or((0, 0));

        // lock held across an await point — deadlock risk under async runtimes
        if has_lock && has_await {
            report.semantic.push(SemanticSmell {
                kind: "lock_across_await".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: "holds mutex/lock across .await — can deadlock async runtime".to_string(),
            });
        }

        // blocking call inside async context — starves the executor
        if has_block_on {
            report.semantic.push(SemanticSmell {
                kind: "sync_in_async".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: "block_on / run_until_complete inside async context starves executor"
                    .to_string(),
            });
        }

        // heavy allocator in a hot symbol
        if alloc_count > 2 && sym.hotspot > 0.7 {
            report.semantic.push(SemanticSmell {
                kind: "alloc_in_hotspot".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: format!(
                    "{} alloc calls in hotspot symbol (hotspot={:.2})",
                    alloc_count, sym.hotspot
                ),
            });
        }

        // high cyclomatic complexity + high fanin + high hotspot — likely a bottleneck
        if sym.complexity > 15 && fanin > 10 && sym.hotspot > 0.6 {
            report.semantic.push(SemanticSmell {
                kind: "high_complexity_bottleneck".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: format!(
                    "complexity={} fanin={} hotspot={:.2} — central and complex",
                    sym.complexity, fanin, sym.hotspot
                ),
            });
        }

        // spawning goroutines/threads inside a tight loop — goroutine leak / thread explosion
        if has_spawn && has_loop {
            report.semantic.push(SemanticSmell {
                kind: "spawn_in_tight_loop".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: "spawns concurrency primitive inside a loop — risk of goroutine/thread explosion".to_string(),
            });
        }

        // very wide fanout, narrow fanin, low churn — abandoned infrastructure
        if fanout > 8 && fanin < 2 && sym.churn < 3 {
            report.semantic.push(SemanticSmell {
                kind: "unbalanced_fanout".to_string(),
                symbol: sym.qualified.clone(),
                file: sym.file.display().to_string(),
                detail: format!(
                    "fanout={} fanin={} churn={} — wide caller with almost no callers, possibly unused",
                    fanout, fanin, sym.churn
                ),
            });
        }
    }
}

const SECURITY_KEYWORDS: &[&str] = &[
    "auth",
    "crypto",
    "token",
    "secret",
    "password",
    "passwd",
    "jwt",
    "oauth",
    "session",
    "credential",
    "key",
    "cert",
    "tls",
    "ssl",
    "permission",
    "acl",
    "policy",
];

fn generate_recommendations(cochange: &[bs_core::CoChange], report: &mut SmellReport) {
    let is_security_file = |path: &str| -> bool {
        let lower = path.to_lowercase();
        SECURITY_KEYWORDS.iter().any(|kw| lower.contains(kw))
    };

    let mut cargo_files: Vec<String> = Vec::new();
    let mut semgrep_files: Vec<String> = Vec::new();

    for c in cochange {
        if c.strength < 0.4 && c.strength_rev < 0.4 {
            continue;
        }
        let a_sec = is_security_file(&c.file_a);
        let b_sec = is_security_file(&c.file_b);
        if a_sec || b_sec {
            let sec_file = if a_sec { &c.file_a } else { &c.file_b };
            let other_file = if a_sec { &c.file_b } else { &c.file_a };

            // Rust dependency files co-changing with security code → cargo audit
            if (other_file.ends_with("Cargo.toml") || other_file.ends_with("Cargo.lock"))
                && !cargo_files.contains(sec_file)
            {
                cargo_files.push(sec_file.clone());
            }

            // Any security-adjacent co-change → semgrep
            if !semgrep_files.contains(sec_file) {
                semgrep_files.push(sec_file.clone());
            }
            if !semgrep_files.contains(other_file) {
                semgrep_files.push(other_file.clone());
            }
        }
    }

    if !cargo_files.is_empty() {
        report.recommendations.push(Recommendation {
            tool: "cargo audit".to_string(),
            reason: "security-sensitive files co-change with Cargo manifests — check for vulnerable deps".to_string(),
            files: cargo_files,
        });
    }

    if !semgrep_files.is_empty() {
        report.recommendations.push(Recommendation {
            tool: "semgrep --config=p/security-audit".to_string(),
            reason: "files with security-keyword paths show strong co-change coupling".to_string(),
            files: semgrep_files,
        });
    }
}

fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((p / 100.0) * (values.len() - 1) as f64).round() as usize;
    values[idx.min(values.len() - 1)]
}

fn semantic_kind_desc(kind: &str) -> &'static str {
    match kind {
        "lock_across_await" => "mutex held across .await — deadlock risk",
        "sync_in_async" => "blocking call inside async fn — starves executor",
        "alloc_in_hotspot" => "heavy allocations in high-churn hot symbol",
        "high_complexity_bottleneck" => "complex + central + hot — hardest to change safely",
        "spawn_in_tight_loop" => "spawning threads/tasks inside a loop — explosion risk",
        "unbalanced_fanout" => "many callees, almost no callers — possibly dead code",
        _ => "structural anomaly",
    }
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
    out.push_str(&format!(
        "stale-core ({} files):\n",
        report.stale_core.len()
    ));
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
