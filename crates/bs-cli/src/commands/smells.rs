use super::{emit, open_store, Context};
use anyhow::Result;
use bs_core::FileStat;
use clap::Args;
use std::collections::HashMap;

#[derive(Args)]
pub struct SmellsArgs {}

pub fn run(ctx: &Context, _args: &SmellsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_all_file_stats()?;
    let cochange = store.get_all_cochange(5)?;

    let mut report = SmellReport::default();
    detect_shotgun_surgery(&cochange, 0.5, 4, &mut report);
    detect_god_file(&stats, &mut report);
    detect_stale_core(&stats, &mut report);
    detect_tangled_pair(&cochange, 0.8, &mut report);

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&report).unwrap_or_default(),
        _ => format_report(&report),
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
}

#[derive(serde::Serialize)]
struct ShotgunEntry {
    file: String,
    partners: usize,
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
            report.shotgun_surgery.push(ShotgunEntry { file, partners: count });
        }
    }
    report.shotgun_surgery.sort_by(|a, b| b.partners.cmp(&a.partners));
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
            report.tangled_pair.push((c.file_a.clone(), c.file_b.clone()));
        }
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

fn format_report(report: &SmellReport) -> String {
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

    out
}
