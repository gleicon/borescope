use bs_core::{Store, Symbol};
use bs_render::{OutputFormat, TreeNode};
use serde::Serialize;
use super::super::Context;

/// A single composable signal emitted for LLM consumption via `--analyze -o json`.
///
/// `kind`: `"path_depth"` | `"lock_await"` | `"blocking_async"` | `"high_complexity"` |
///         `"hot_symbol"` | `"cross_file_boundary"`
/// `severity`: `"info"` | `"medium"` | `"high"`
/// `detail`: human-readable explanation suitable for direct LLM prompt injection.
#[derive(Serialize, Clone)]
pub(super) struct Signal {
    pub kind: String,
    pub severity: String,
    pub detail: String,
}

/// Analyze a list of symbols on the call path and return LLM-legible signals.
pub(super) fn analyze_symbols(syms: &[Symbol], store: &Store) -> Vec<Signal> {
    let mut signals: Vec<Signal> = Vec::new();

    if syms.is_empty() {
        return signals;
    }

    signals.extend(signals_for_path_shape(syms));
    signals.extend(signals_per_symbol(syms));
    signals.extend(signals_for_terminal(syms, store));

    signals
}

fn signals_for_path_shape(syms: &[Symbol]) -> Vec<Signal> {
    let mut out = Vec::new();
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    out.push(Signal {
        kind: "path_depth".into(),
        severity: if syms.len() > 5 { "medium" } else { "info" }.into(),
        detail: format!(
            "call chain has {} frame{}: {}",
            syms.len(),
            if syms.len() == 1 { "" } else { "s" },
            names.join(" → ")
        ),
    });

    let mut seen_files: Vec<String> = Vec::new();
    for f in syms.iter().map(|s| s.file.to_string_lossy().into_owned()) {
        if !seen_files.contains(&f) {
            seen_files.push(f);
        }
    }
    if seen_files.len() > 1 {
        out.push(Signal {
            kind: "cross_file_boundary".into(),
            severity: "info".into(),
            detail: format!(
                "path crosses {} file{}: {}",
                seen_files.len(),
                if seen_files.len() == 1 { "" } else { "s" },
                seen_files.join(" → ")
            ),
        });
    }
    out
}

fn signals_per_symbol(syms: &[Symbol]) -> Vec<Signal> {
    let mut out = Vec::new();
    for sym in syms {
        let has_lock = sym.patterns.contains(&"lock".to_string());
        let has_await = sym.patterns.contains(&"await".to_string());
        let has_block_on = sym.patterns.contains(&"block_on".to_string());
        let has_loop = sym.patterns.contains(&"loop".to_string());
        let loc = sym.file.to_string_lossy();
        let at = format!("{}:{}", loc, sym.span.0);

        if has_lock && has_await {
            out.push(Signal {
                kind: "lock_await".into(),
                severity: "high".into(),
                detail: format!(
                    "`{}` at {} holds a lock while awaiting an async operation — \
                     deadlock risk if the awaited future tries to acquire the same lock \
                     or if executor threads are exhausted",
                    sym.name, at
                ),
            });
        }
        if has_block_on {
            out.push(Signal {
                kind: "blocking_async".into(),
                severity: "high".into(),
                detail: format!(
                    "`{}` at {} calls block_on() — synchronously blocks the current thread \
                     waiting for an async future; risks thread starvation under concurrent load",
                    sym.name, at
                ),
            });
        }
        if has_loop {
            out.push(Signal {
                kind: "unbounded_loop".into(),
                severity: "medium".into(),
                detail: format!(
                    "`{}` at {} contains a loop — verify the iteration bound is proportional \
                     to input size; an O(n) or O(n²) loop on a hot path becomes a bottleneck \
                     when request volume grows 10×",
                    sym.name, at
                ),
            });
        }
        if sym.complexity > 10 {
            out.push(Signal {
                kind: "high_complexity".into(),
                severity: "medium".into(),
                detail: format!(
                    "`{}` at {} has cyclomatic complexity {} — high branch count \
                     increases the number of untested execution paths and makes \
                     performance regressions harder to isolate",
                    sym.name, at, sym.complexity
                ),
            });
        }
        if sym.hotspot > 0.7 {
            out.push(Signal {
                kind: "hot_symbol".into(),
                severity: "medium".into(),
                detail: format!(
                    "`{}` at {} is a hotspot (score {:.2}) — \
                     frequently changed code on the call path; higher probability of \
                     latent bugs introduced under deadline pressure",
                    sym.name, at, sym.hotspot
                ),
            });
        }
    }
    out
}

fn signals_for_terminal(syms: &[Symbol], store: &Store) -> Vec<Signal> {
    let mut out = Vec::new();
    let Some(last) = syms.last() else { return out };

    let has_spawn = last.patterns.contains(&"spawn".to_string());
    let has_chan = last.patterns.contains(&"chan".to_string());
    if has_spawn || has_chan {
        let mechanism = if has_spawn { "task spawn" } else { "channel send" };
        out.push(Signal {
            kind: "async_handoff".into(),
            severity: "info".into(),
            detail: format!(
                "path terminates at `{}` which performs a {} — the consumer \
                 runs asynchronously; trace `borescope paths <consumer>` to \
                 follow the continuation",
                last.name, mechanism
            ),
        });
    }

    if let Ok(externals) = store.get_external_callees(&last.id) {
        if !externals.is_empty() {
            out.push(Signal {
                kind: "external_boundary".into(),
                severity: "info".into(),
                detail: format!(
                    "path terminates at external calls not in the indexed codebase: {} — \
                     latency and failure modes here depend on runtime behavior, \
                     not static analysis",
                    externals.join(", ")
                ),
            });
        }
    }
    out
}

pub(super) fn render_path_output(
    ctx: &Context,
    from: &str,
    to: &str,
    root_node: TreeNode,
    signals: Option<Vec<Signal>>,
) -> anyhow::Result<String> {
    match ctx.output {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct PathJson {
                borescope: &'static str,
                schema: u32,
                from: String,
                to: String,
                path: Option<TreeNode>,
                signals: Vec<Signal>,
            }
            let out = PathJson {
                borescope: env!("CARGO_PKG_VERSION"),
                schema: 1,
                from: from.to_string(),
                to: to.to_string(),
                path: Some(root_node),
                signals: signals.unwrap_or_default(),
            };
            Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
        }
        OutputFormat::Mermaid => {
            let mut out = bs_render::mermaid::render_sequence(&root_node, ctx.no_fence);
            if let Some(sigs) = signals {
                out.push_str(&render_signals_text(&sigs));
            }
            Ok(out)
        }
        OutputFormat::Dot => {
            let mut out = bs_render::dot::render_sequence(&root_node, ctx.no_fence);
            if let Some(sigs) = signals {
                out.push_str(&render_signals_text(&sigs));
            }
            Ok(out)
        }
        _ => {
            let mut out = bs_render::tree::render_tree(&[root_node], ctx.no_color, Some(ctx.depth as usize));
            if let Some(sigs) = signals {
                out.push_str(&render_signals_text(&sigs));
            }
            Ok(out)
        }
    }
}

pub(super) fn render_analyze_output(
    ctx: &Context,
    target: &str,
    root_node: &TreeNode,
    signals: &[Signal],
) -> String {
    match ctx.output {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct AnalyzeJson<'a> {
                borescope: &'static str,
                schema: u32,
                target: &'a str,
                signals: &'a [Signal],
            }
            let out = AnalyzeJson {
                borescope: env!("CARGO_PKG_VERSION"),
                schema: 1,
                target,
                signals,
            };
            serde_json::to_string_pretty(&out).unwrap_or_default()
        }
        _ => {
            let mut out = bs_render::tree::render_tree(
                std::slice::from_ref(root_node),
                ctx.no_color,
                Some(ctx.depth as usize),
            );
            out.push_str(&render_signals_text(signals));
            out
        }
    }
}

pub(super) fn render_signals_text(signals: &[Signal]) -> String {
    if signals.is_empty() {
        return "\n(no signals)\n".to_string();
    }
    let mut out = String::from("\nsignals:\n");
    for sig in signals {
        let sev = sig.severity.to_uppercase();
        let pad = "  ";
        out.push_str(&format!("\n  [{}]  {}\n", sev, sig.kind));
        for word_line in wrap_text(&sig.detail, 72) {
            out.push_str(&format!("{}  {}\n", pad, word_line));
        }
    }
    out
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + word.len() + 1 > width {
            lines.push(current.trim().to_string());
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.trim().is_empty() {
        lines.push(current.trim().to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::{model::SymbolKind, LangId, Store};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn tmp_store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn make_sym(id: &str, name: &str, file: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qualified: format!("{}:{}", file, name),
            file: PathBuf::from(file),
            span: (1, 10),
            lang: LangId::Rust,
            churn: 0,
            age_days: 0,
            loc: 10,
            complexity: 1,
            hotspot: 0.0,
            patterns: vec![],
        }
    }

    #[test]
    fn analyze_empty_returns_no_signals() {
        let (_dir, store) = tmp_store();
        assert!(analyze_symbols(&[], &store).is_empty());
    }

    #[test]
    fn analyze_emits_path_depth_signal() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let sigs = analyze_symbols(&[a], &store);
        assert!(sigs.iter().any(|s| s.kind == "path_depth"));
    }

    #[test]
    fn analyze_emits_lock_await_signal() {
        let (_dir, store) = tmp_store();
        let mut sym = make_sym("a", "risky", "src/a.rs");
        sym.patterns = vec!["lock".to_string(), "await".to_string()];
        let sigs = analyze_symbols(&[sym], &store);
        let found = sigs.iter().find(|s| s.kind == "lock_await");
        assert!(found.is_some());
        assert_eq!(found.unwrap().severity, "high");
    }

    #[test]
    fn analyze_emits_blocking_async_signal() {
        let (_dir, store) = tmp_store();
        let mut sym = make_sym("a", "blocker", "src/a.rs");
        sym.patterns = vec!["block_on".to_string()];
        let sigs = analyze_symbols(&[sym], &store);
        assert!(sigs
            .iter()
            .any(|s| s.kind == "blocking_async" && s.severity == "high"));
    }

    #[test]
    fn analyze_emits_high_complexity_signal() {
        let (_dir, store) = tmp_store();
        let mut sym = make_sym("a", "complex", "src/a.rs");
        sym.complexity = 15;
        let sigs = analyze_symbols(&[sym], &store);
        assert!(sigs.iter().any(|s| s.kind == "high_complexity"));
    }

    #[test]
    fn analyze_emits_hot_symbol_signal() {
        let (_dir, store) = tmp_store();
        let mut sym = make_sym("a", "hot", "src/a.rs");
        sym.hotspot = 0.9;
        let sigs = analyze_symbols(&[sym], &store);
        assert!(sigs
            .iter()
            .any(|s| s.kind == "hot_symbol" && s.severity == "medium"));
    }

    #[test]
    fn analyze_emits_cross_file_boundary() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        let sigs = analyze_symbols(&[a, b], &store);
        assert!(sigs.iter().any(|s| s.kind == "cross_file_boundary"));
    }

    #[test]
    fn analyze_no_cross_file_signal_same_file() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/a.rs");
        let sigs = analyze_symbols(&[a, b], &store);
        assert!(!sigs.iter().any(|s| s.kind == "cross_file_boundary"));
    }
}
