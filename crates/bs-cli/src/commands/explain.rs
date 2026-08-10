use super::{emit, has_pattern, open_store, resolve_target, Context};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct ExplainArgs {
    /// Symbol name, file:name, or file:line
    pub target: String,
}

pub fn run(ctx: &Context, args: &ExplainArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let sym = resolve_target(&store, &args.target)?;

    let edge_counts = store.get_call_edge_counts().unwrap_or_default();
    let (fanin, fanout) = edge_counts
        .get(&sym.id.to_string())
        .copied()
        .unwrap_or((0, 0));

    let file_str = sym.file.to_string_lossy().to_string();
    let all_cochange = store.get_all_cochange(5).unwrap_or_default();
    let cochange: Vec<_> = all_cochange
        .into_iter()
        .filter(|c| c.file_a == file_str || c.file_b == file_str)
        .collect();

    let out = if ctx.output == bs_render::OutputFormat::Json {
        serde_json::to_string_pretty(&ExplainJson {
            symbol: sym.qualified.clone(),
            file: sym.file.display().to_string(),
            kind: sym.kind.to_string(),
            span: sym.span,
            loc: sym.loc,
            complexity: sym.complexity,
            churn: sym.churn,
            hotspot: sym.hotspot,
            fanin,
            fanout,
            patterns: sym.patterns.clone(),
            cochange_partners: cochange
                .iter()
                .map(|c| {
                    if c.file_a == file_str {
                        c.file_b.clone()
                    } else {
                        c.file_a.clone()
                    }
                })
                .collect(),
        })
        .unwrap_or_default()
    } else {
        narrative(&sym, fanin, fanout, &cochange, &file_str)
    };

    emit(ctx, &out);
    Ok(())
}

fn narrative(
    sym: &bs_core::Symbol,
    fanin: u32,
    fanout: u32,
    cochange: &[bs_core::CoChange],
    file_str: &str,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("=== {} ===", sym.qualified));
    lines.push(format!(
        "  file    : {}  (lines {}–{})",
        sym.file.display(),
        sym.span.0,
        sym.span.1
    ));
    lines.push(format!(
        "  kind    : {}  loc: {}  complexity: {}",
        sym.kind, sym.loc, sym.complexity
    ));
    lines.push(String::new());

    let heat = match sym.hotspot {
        h if h >= 0.8 => "🔥 very hot — changes constantly and recently",
        h if h >= 0.5 => "warm — moderate churn, somewhat recent",
        h if h >= 0.2 => "lukewarm — changes occasionally",
        _ => "cold — rarely touched",
    };
    lines.push(format!(
        "  hotspot : {:.2}  churn: {} commits  → {}",
        sym.hotspot, sym.churn, heat
    ));

    let complexity_note = match sym.complexity {
        c if c > 20 => "very high — hard to reason about, test coverage critical",
        c if c > 10 => "high — multiple code paths, review carefully before changing",
        c if c > 5 => "moderate",
        _ => "low",
    };
    lines.push(format!(
        "  complexity: {}  → {}",
        sym.complexity, complexity_note
    ));

    lines.push(String::new());
    let fanin_note = match fanin {
        0 => "nothing calls this — possibly dead code or an entry point",
        1 => "called from one place — low blast radius",
        2..=5 => "called from a few places",
        _ => "called from many places — changes ripple widely",
    };
    let fanout_note = match fanout {
        0 => "calls nothing — leaf function",
        1..=3 => "calls a few things",
        4..=8 => "moderate dependencies",
        _ => "calls many things — wide dependency surface",
    };
    lines.push(format!("  fanin  : {}  → {}", fanin, fanin_note));
    lines.push(format!("  fanout : {}  → {}", fanout, fanout_note));

    // Semantic patterns
    if !sym.patterns.is_empty() {
        lines.push(String::new());
        lines.push(format!("  patterns: {}", sym.patterns.join(", ")));
        let has_lock = has_pattern(&sym.patterns, "lock");
        let has_await = has_pattern(&sym.patterns, "await");
        let has_block = has_pattern(&sym.patterns, "block_on");
        let has_spawn = has_pattern(&sym.patterns, "spawn");
        let has_loop = has_pattern(&sym.patterns, "loop");
        if has_lock && has_await {
            lines.push(
                "  ⚠ lock held across .await — risk of deadlock under async runtime".to_string(),
            );
        }
        if has_block {
            lines.push("  ⚠ block_on inside async context — will starve the executor".to_string());
        }
        if has_spawn && has_loop {
            lines.push(
                "  ⚠ spawning inside a loop — risk of goroutine/thread explosion".to_string(),
            );
        }
    }

    if !cochange.is_empty() {
        lines.push(String::new());
        lines.push("  co-changes with:".to_string());
        for c in cochange.iter().take(5) {
            let partner = if c.file_a == file_str {
                &c.file_b
            } else {
                &c.file_a
            };
            let strength = c.strength.max(c.strength_rev);
            let coupling = match strength {
                s if s >= 0.8 => "always together",
                s if s >= 0.5 => "often together",
                _ => "sometimes together",
            };
            lines.push(format!(
                "    {:.0}%  {}  ({})",
                strength * 100.0,
                partner,
                coupling
            ));
        }
        lines.push(
            "  → if you change this symbol, those files likely need updating too".to_string(),
        );
    }

    lines.push(String::new());
    lines.push("  verdict:".to_string());
    let risk = risk_level(sym, fanin);
    lines.push(format!("    {}", risk));

    lines.join("\n")
}

fn risk_level(sym: &bs_core::Symbol, fanin: u32) -> &'static str {
    let hot = sym.hotspot > 0.6;
    let complex = sym.complexity > 10;
    let central = fanin > 5;
    let dangerous_pattern =
        has_pattern(&sym.patterns, "lock") && has_pattern(&sym.patterns, "await");

    match (hot, complex, central, dangerous_pattern) {
        (_, _, _, true) => "HIGH RISK — dangerous concurrency pattern detected",
        (true, true, true, _) => {
            "HIGH RISK — hot, complex, and central; changes here are expensive"
        }
        (true, true, false, _) => "MEDIUM RISK — hot and complex, but limited blast radius",
        (true, false, true, _) => "MEDIUM RISK — hot and central; keep changes minimal",
        (false, true, true, _) => "MEDIUM RISK — complex and central; refactor carefully",
        _ => "LOW RISK — cold, simple, or isolated",
    }
}

#[derive(serde::Serialize)]
struct ExplainJson {
    symbol: String,
    file: String,
    kind: String,
    span: (u32, u32),
    loc: u32,
    complexity: u32,
    churn: u32,
    hotspot: f32,
    fanin: u32,
    fanout: u32,
    patterns: Vec<String>,
    cochange_partners: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::risk_level;
    use bs_core::{LangId, Symbol, SymbolKind};
    use std::path::PathBuf;

    fn sym(hotspot: f32, complexity: u32, _fanin: u32, patterns: &[&str]) -> Symbol {
        Symbol {
            id: "test".to_string(),
            kind: SymbolKind::Function,
            name: "test".to_string(),
            qualified: "test".to_string(),
            file: PathBuf::from("src/lib.rs"),
            span: (1, 10),
            lang: LangId::Rust,
            churn: 0,
            age_days: 0,
            loc: 10,
            complexity,
            hotspot,
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_risk_dangerous_pattern_always_high() {
        let s = sym(0.0, 0, 0, &["lock", "await"]);
        assert!(risk_level(&s, 0).starts_with("HIGH RISK"));
    }

    #[test]
    fn test_risk_hot_complex_central_is_high() {
        let s = sym(0.8, 15, 10, &[]);
        assert!(risk_level(&s, 10).starts_with("HIGH RISK"));
    }

    #[test]
    fn test_risk_hot_and_complex_medium() {
        let s = sym(0.8, 15, 0, &[]);
        assert!(risk_level(&s, 0).starts_with("MEDIUM RISK"));
    }

    #[test]
    fn test_risk_hot_and_central_medium() {
        let s = sym(0.8, 3, 10, &[]);
        assert!(risk_level(&s, 10).starts_with("MEDIUM RISK"));
    }

    #[test]
    fn test_risk_complex_and_central_medium() {
        let s = sym(0.0, 15, 10, &[]);
        assert!(risk_level(&s, 10).starts_with("MEDIUM RISK"));
    }

    #[test]
    fn test_risk_cold_simple_low() {
        let s = sym(0.1, 2, 1, &[]);
        assert!(risk_level(&s, 1).starts_with("LOW RISK"));
    }
}
