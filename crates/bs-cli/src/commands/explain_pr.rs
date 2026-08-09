use super::{emit, open_store, Context};
use anyhow::Result;
use bs_core::Symbol;
use bs_git::Miner;
use clap::Args;
use std::collections::{HashMap, HashSet};

#[derive(Args)]
pub struct ExplainPrArgs {
    /// Branch name to explain
    pub branch: String,

    /// Base branch to diff against [default: main]
    #[arg(long, default_value = "main")]
    pub base: String,
}

pub fn run(ctx: &Context, args: &ExplainPrArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let miner = Miner::new(ctx.repo_root.clone());

    // Use three-dot diff: shows what's on branch relative to merge-base with base.
    // Falls back to two-dot if three-dot yields nothing (branch already in base).
    let changed_files = {
        let merge_base = miner.merge_base(&args.base, &args.branch).unwrap_or_default();
        let via_base = if !merge_base.is_empty() {
            miner.changed_files(&merge_base, &args.branch).unwrap_or_default()
        } else {
            vec![]
        };
        if via_base.is_empty() {
            // Branch already merged or is behind — show what differs between tips
            miner.changed_files(&args.base, &args.branch).unwrap_or_default()
        } else {
            via_base
        }
    };

    if changed_files.is_empty() {
        emit(ctx, "No changed files found between branch and base.");
        return Ok(());
    }

    // Collect all symbols in changed files + their signals
    let edge_counts = store.get_call_edge_counts().unwrap_or_default();
    let all_cochange = store.get_all_cochange(3).unwrap_or_default();

    let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();

    let mut all_syms: Vec<(Symbol, u32, u32)> = Vec::new(); // (sym, fanin, fanout)
    for file in &changed_files {
        let syms = store.symbols_for_file(file).unwrap_or_default();
        for sym in syms {
            let (fi, fo) = edge_counts
                .get(&sym.id.to_string())
                .copied()
                .unwrap_or((0, 0));
            all_syms.push((sym, fi, fo));
        }
    }

    // Co-change warnings: files that typically move with changed files but are NOT in this PR
    let mut missed_partners: HashMap<String, f32> = HashMap::new();
    for c in &all_cochange {
        let a_in = changed_set.contains(c.file_a.as_str());
        let b_in = changed_set.contains(c.file_b.as_str());
        let strength = c.strength.max(c.strength_rev);
        if strength < 0.5 {
            continue;
        }
        if a_in && !b_in {
            let e = missed_partners.entry(c.file_b.clone()).or_default();
            if strength > *e { *e = strength; }
        } else if b_in && !a_in {
            let e = missed_partners.entry(c.file_a.clone()).or_default();
            if strength > *e { *e = strength; }
        }
    }
    let mut missed: Vec<(String, f32)> = missed_partners.into_iter().collect();
    missed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let out = if ctx.output == bs_render::OutputFormat::Json {
        render_json(&args.branch, &args.base, &changed_files, &all_syms, &missed)
    } else {
        render_text(&args.branch, &args.base, &changed_files, &all_syms, &missed)
    };

    emit(ctx, &out);
    Ok(())
}

fn render_text(
    branch: &str,
    base: &str,
    changed_files: &[String],
    syms: &[(Symbol, u32, u32)],
    missed_partners: &[(String, f32)],
) -> String {
    let mut out = String::new();

    out.push_str(&format!("=== PR impact: {} → {} ===\n\n", branch, base));
    out.push_str(&format!("  {} files changed  |  {} symbols touched\n\n",
        changed_files.len(), syms.len()));

    // Changed files — cap display at 20 to avoid noise for large PRs
    out.push_str("changed files:\n");
    for f in changed_files.iter().take(20) {
        out.push_str(&format!("  {}\n", f));
    }
    if changed_files.len() > 20 {
        out.push_str(&format!("  … and {} more\n", changed_files.len() - 20));
    }

    // High-risk: must have real signals — pure hotspot with complexity=0 means new/trivial file
    let high_risk: Vec<_> = syms.iter().filter(|(s, fi, _)| {
        let dangerous = s.patterns.iter().any(|p| p == "lock") && s.patterns.iter().any(|p| p == "await");
        dangerous
            || (s.hotspot > 0.5 && s.complexity > 8)
            || (s.hotspot > 0.7 && s.complexity > 3)
            || (*fi > 8 && s.complexity > 3)
    }).collect();

    out.push_str(&format!("\nhigh-risk symbols ({}):\n", high_risk.len()));
    if high_risk.is_empty() {
        out.push_str("  (none — looks safe)\n");
    }
    let shown_high = high_risk.len().min(20);
    for (s, fi, fo) in high_risk.iter().take(shown_high) {
        let flags: Vec<&str> = [
            if s.hotspot > 0.7 { Some("hot") } else { None },
            if s.complexity > 10 { Some("complex") } else { None },
            if *fi > 8 { Some("central") } else { None },
            if s.patterns.iter().any(|p| p == "lock") && s.patterns.iter().any(|p| p == "await") {
                Some("⚠ lock+await")
            } else { None },
            if s.patterns.iter().any(|p| p == "block_on") { Some("⚠ block_on") } else { None },
        ].iter().flatten().copied().collect();

        out.push_str(&format!(
            "  {} [{}]\n    hotspot:{:.2}  complexity:{}  fanin:{}  fanout:{}\n    flags: {}\n",
            s.qualified, s.kind,
            s.hotspot, s.complexity, fi, fo,
            if flags.is_empty() { "—".to_string() } else { flags.join(", ") }
        ));
    }
    if high_risk.len() > 20 {
        out.push_str(&format!("  … and {} more (use -o json for full list)\n", high_risk.len() - 20));
    }

    // Co-change warnings
    out.push_str(&format!("\nco-change warnings ({} files usually move with these but are NOT in this PR):\n",
        missed_partners.len()));
    if missed_partners.is_empty() {
        out.push_str("  (none)\n");
    }
    for (file, strength) in missed_partners.iter().take(8) {
        out.push_str(&format!("  {:.0}%  {}\n", strength * 100.0, file));
    }
    if missed_partners.len() > 8 {
        out.push_str(&format!("  … and {} more\n", missed_partners.len() - 8));
    }

    // Semantic patterns in the diff
    let pattern_counts = pattern_summary(syms);
    if !pattern_counts.is_empty() {
        out.push_str("\nsemantic patterns in touched code:\n");
        let mut sorted: Vec<_> = pattern_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (pat, count) in sorted {
            out.push_str(&format!("  {:<12}  {} symbols\n", pat, count));
        }
    }

    // Verdict
    out.push_str("\nverdict:\n");
    let n_high = high_risk.len();
    let n_missed = missed_partners.len();
    let has_dangerous = syms.iter().any(|(s, _, _)| {
        s.patterns.iter().any(|p| p == "lock") && s.patterns.iter().any(|p| p == "await")
    });
    if has_dangerous || n_high > 5 {
        out.push_str("  HIGH RISK — review high-risk symbols carefully before merge\n");
    } else if n_high > 0 || n_missed > 3 {
        out.push_str("  MEDIUM RISK — some hot/complex symbols touched; check co-change warnings\n");
    } else {
        out.push_str("  LOW RISK — no hot or complex symbols touched\n");
    }
    if n_missed > 0 {
        out.push_str(&format!("  {} likely-related files not in PR — intentional?\n", n_missed));
    }

    out
}

fn pattern_summary(syms: &[(Symbol, u32, u32)]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (s, _, _) in syms {
        for p in &s.patterns {
            *counts.entry(p.clone()).or_default() += 1;
        }
    }
    counts
}

fn render_json(
    branch: &str,
    base: &str,
    changed_files: &[String],
    syms: &[(Symbol, u32, u32)],
    missed_partners: &[(String, f32)],
) -> String {
    #[derive(serde::Serialize)]
    struct Out<'a> {
        branch: &'a str,
        base: &'a str,
        changed_files: &'a [String],
        symbols_touched: usize,
        high_risk: Vec<SymEntry<'a>>,
        missed_cochange: Vec<(&'a str, f32)>,
        pattern_counts: HashMap<String, usize>,
    }
    #[derive(serde::Serialize)]
    struct SymEntry<'a> {
        qualified: &'a str,
        hotspot: f32,
        complexity: u32,
        fanin: u32,
        patterns: &'a [String],
    }

    let high_risk: Vec<SymEntry> = syms.iter().filter(|(s, fi, _)| {
        let dangerous = s.patterns.iter().any(|p| p == "lock") && s.patterns.iter().any(|p| p == "await");
        dangerous || (s.hotspot > 0.5 && s.complexity > 8) || (s.hotspot > 0.7) || (*fi > 8)
    }).map(|(s, fi, _)| SymEntry {
        qualified: &s.qualified,
        hotspot: s.hotspot,
        complexity: s.complexity,
        fanin: *fi,
        patterns: &s.patterns,
    }).collect();

    let o = Out {
        branch,
        base,
        changed_files,
        symbols_touched: syms.len(),
        high_risk,
        missed_cochange: missed_partners.iter().map(|(f, s)| (f.as_str(), *s)).collect(),
        pattern_counts: pattern_summary(syms),
    };
    serde_json::to_string_pretty(&o).unwrap_or_default()
}
