use super::has_pattern;
use bs_core::Symbol;

/// True when patterns contain both "lock" and "await" — deadlock under async runtime.
pub fn is_dangerous(patterns: &[String]) -> bool {
    has_pattern(patterns, "lock") && has_pattern(patterns, "await")
}

pub fn risk_level(sym: &Symbol, fanin: u32) -> &'static str {
    let hot = sym.hotspot > 0.6;
    let complex = sym.complexity > 10;
    let central = fanin > 5;
    let dangerous = is_dangerous(&sym.patterns);

    match (hot, complex, central, dangerous) {
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

/// High-risk predicate for PR-impact filtering.
/// Broader than `risk_level` — catches hot-but-simple symbols central to many callers.
pub fn is_high_risk_pr(sym: &Symbol, fanin: u32) -> bool {
    is_dangerous(&sym.patterns)
        || (sym.hotspot > 0.5 && sym.complexity > 8)
        || (sym.hotspot > 0.7 && sym.complexity > 3)
        || (fanin > 8 && sym.complexity > 3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::{LangId, SymbolKind};

    fn sym(hotspot: f32, complexity: u32, patterns: Vec<String>) -> Symbol {
        Symbol {
            id: "id".to_string(),
            kind: SymbolKind::Function,
            name: "fn".to_string(),
            qualified: "fn".to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            span: (1, 10),
            lang: LangId::Rust,
            churn: 0,
            age_days: 0,
            loc: 10,
            complexity,
            hotspot,
            patterns,
        }
    }

    fn dangerous() -> Vec<String> {
        vec!["lock".to_string(), "await".to_string()]
    }

    #[test]
    fn test_risk_level_dangerous_pattern_is_always_high() {
        let s = sym(0.0, 0, dangerous());
        assert!(risk_level(&s, 0).starts_with("HIGH RISK"));
        assert!(risk_level(&s, 0).contains("dangerous"));
    }

    #[test]
    fn test_risk_level_hot_complex_central_is_high() {
        let s = sym(0.8, 15, vec![]);
        assert!(risk_level(&s, 6).starts_with("HIGH RISK"));
    }

    #[test]
    fn test_risk_level_cold_simple_is_low() {
        let s = sym(0.1, 3, vec![]);
        assert_eq!(risk_level(&s, 1), "LOW RISK — cold, simple, or isolated");
    }

    #[test]
    fn test_is_high_risk_pr_dangerous_pattern() {
        let s = sym(0.0, 0, dangerous());
        assert!(is_high_risk_pr(&s, 0));
    }

    #[test]
    fn test_is_high_risk_pr_high_fanin_with_complexity() {
        // fanin > 8 && complexity > 3
        let s = sym(0.0, 5, vec![]);
        assert!(is_high_risk_pr(&s, 9));
    }

    #[test]
    fn test_is_high_risk_pr_false_for_cold_simple() {
        let s = sym(0.1, 2, vec![]);
        assert!(!is_high_risk_pr(&s, 2));
    }
}
