use bs_core::{CoChange, FileStat, Symbol};
use crate::thresholds::{CustomSmellRule, Thresholds};
use std::collections::HashMap;
use super::{has_pattern, SmellReport, SemanticSmell, ShotgunEntry, Recommendation};
use std::cmp::Reverse;

pub(super) fn detect_shotgun_surgery(
    cochange: &[CoChange],
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
            report.shotgun_surgery.push(ShotgunEntry { file, partners: count });
        }
    }
    report.shotgun_surgery.sort_by_key(|e| Reverse(e.partners));
}

pub(super) fn detect_god_file(stats: &[FileStat], report: &mut SmellReport) {
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

pub(super) fn detect_stale_core(stats: &[FileStat], report: &mut SmellReport) {
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

pub(super) fn detect_tangled_pair(
    cochange: &[CoChange],
    min_strength: f32,
    report: &mut SmellReport,
) {
    for c in cochange {
        if c.strength >= min_strength && c.strength_rev >= min_strength {
            report.tangled_pair.push((c.file_a.clone(), c.file_b.clone()));
        }
    }
}

pub(super) fn detect_semantic(
    symbols: &[Symbol],
    edge_counts: &HashMap<String, (u32, u32)>,
    t: &Thresholds,
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
        let (fanin, fanout) = edge_counts.get(&sym.id.to_string()).copied().unwrap_or((0, 0));

        if has_lock && has_await {
            push_semantic(report, "lock_across_await", sym,
                "holds mutex/lock across .await — can deadlock async runtime".to_string());
        }
        if has_block_on {
            push_semantic(report, "sync_in_async", sym,
                "block_on / run_until_complete inside async context starves executor".to_string());
        }
        if alloc_count > 2 && sym.hotspot > t.hotspot_high {
            push_semantic(report, "alloc_in_hotspot", sym,
                format!("{} alloc calls in hotspot symbol (hotspot={:.2})", alloc_count, sym.hotspot));
        }
        if sym.complexity > t.complexity_absolute {
            push_semantic(report, "structural_violation", sym,
                format!("complexity={} exceeds absolute limit {} — split this function",
                    sym.complexity, t.complexity_absolute));
        }
        if sym.loc > t.loc_high {
            push_semantic(report, "structural_violation", sym,
                format!("loc={} exceeds limit {} — too many responsibilities in one function",
                    sym.loc, t.loc_high));
        }
        if sym.complexity > t.complexity_high && fanin > t.fanin_high && sym.hotspot > t.hotspot_medium {
            push_semantic(report, "high_complexity_bottleneck", sym,
                format!("complexity={} fanin={} hotspot={:.2} — central and complex",
                    sym.complexity, fanin, sym.hotspot));
        }
        if has_spawn && has_loop {
            push_semantic(report, "spawn_in_tight_loop", sym,
                "spawns concurrency primitive inside a loop — risk of goroutine/thread explosion".to_string());
        }
        if fanout > 8 && fanin < 2 && sym.churn < 3 {
            push_semantic(report, "unbalanced_fanout", sym,
                format!("fanout={} fanin={} churn={} — wide caller with almost no callers, possibly unused",
                    fanout, fanin, sym.churn));
        }
    }
}

fn push_semantic(report: &mut SmellReport, kind: &str, sym: &Symbol, detail: String) {
    report.semantic.push(SemanticSmell {
        kind: kind.to_string(),
        symbol: sym.qualified.clone(),
        file: sym.file.display().to_string(),
        detail,
    });
}

/// Apply user-defined pattern combination rules from `.borescope/smells.toml`.
pub(super) fn detect_custom_rules(
    symbols: &[Symbol],
    rules: &[CustomSmellRule],
    report: &mut SmellReport,
) {
    if rules.is_empty() {
        return;
    }
    for sym in symbols {
        if sym.patterns.is_empty() {
            continue;
        }
        for rule in rules {
            if rule.patterns.iter().all(|p| sym.patterns.contains(p)) {
                report.semantic.push(SemanticSmell {
                    kind: rule.name.clone(),
                    symbol: sym.qualified.clone(),
                    file: sym.file.display().to_string(),
                    detail: format!("[{}] {}", rule.severity, rule.description),
                });
            }
        }
    }
}

const SECURITY_KEYWORDS: &[&str] = &[
    "auth", "crypto", "token", "secret", "password", "passwd", "jwt",
    "oauth", "session", "credential", "key", "cert", "tls", "ssl",
    "permission", "acl", "policy",
];

pub(super) fn generate_recommendations(
    cochange: &[CoChange],
    report: &mut SmellReport,
) {
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
            if (other_file.ends_with("Cargo.toml") || other_file.ends_with("Cargo.lock"))
                && !cargo_files.contains(sec_file)
            {
                cargo_files.push(sec_file.clone());
            }
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

pub(super) fn percentile(mut values: Vec<f64>, p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let idx = ((p / 100.0) * (values.len() - 1) as f64).round() as usize;
    values[idx.min(values.len() - 1)]
}
