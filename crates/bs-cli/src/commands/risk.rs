use super::has_pattern;
use bs_core::Symbol;

/// True when patterns contain both "lock" and "await" — deadlock under async runtime.
pub fn is_dangerous(patterns: &[String]) -> bool {
    has_pattern(patterns, "lock") && has_pattern(patterns, "await")
}

/// Single-symbol risk verdict for `explain` output.
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
