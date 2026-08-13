use super::{build_tree_node, emit, open_store, resolve_target, Context};
use anyhow::Result;
use bs_core::{Store, Symbol};
use bs_render::{self, folded, html, json, tree, OutputFormat, TreeNode};
use clap::Args;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};

#[derive(Args)]
pub struct PathsArgs {
    /// Start symbol: path/to/file.go:FuncName | path/to/file.go:42 | QualifiedName
    pub target: String,

    /// Find shortest call path to this target symbol (BFS through call graph)
    #[arg(long, value_name = "TARGET")]
    pub to: Option<String>,

    /// Emit LLM-legible signal analysis of the call path
    #[arg(long)]
    pub analyze: bool,
}

pub fn run(ctx: &Context, args: &PathsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let sym = resolve_target(&store, &args.target)?;

    let all_syms = store.all_symbols()?;
    let max_churn = all_syms.iter().map(|s| s.churn as f32).fold(0.0f32, f32::max);
    let max_loc = all_syms.iter().map(|s| s.loc as f32).fold(0.0f32, f32::max);

    // --to mode — BFS to find shortest path to a target symbol
    if let Some(ref to_target) = args.to {
        let end_sym = resolve_target(&store, to_target)?;
        let path = find_path_to(&store, &sym, &end_sym, ctx.depth, ctx.min_confidence);

        let (root_node, path_syms) = if let Some(path) = path {
            // Build a linear tree from the path: each hop is a child of the previous
            let path_syms = enrich_with_patterns(&store, &path);
            let root = build_path_tree(&path_syms, max_churn, max_loc, ctx.weight);
            (root, path_syms)
        } else {
            anyhow::bail!(
                "no path found from '{}' to '{}' within depth {}",
                args.target,
                to_target,
                ctx.depth
            );
        };

        let signals = if args.analyze {
            Some(analyze_symbols(&path_syms, &store))
        } else {
            None
        };

        let out = render_path_output(ctx, &args.target, to_target, root_node, signals, &store)?;
        emit(ctx, &out);
        return Ok(());
    }

    // Normal mode — full call tree from start symbol
    let mut visited = HashSet::new();
    let root_node = build_tree_node(
        &store,
        &sym,
        0,
        ctx.depth,
        ctx.min_confidence,
        1.0,
        ctx.weight,
        max_churn,
        max_loc,
        &mut visited,
    );

    if args.analyze {
        // Collect all reachable symbols from the tree and analyze them
        let reachable_ids: Vec<String> = collect_tree_ids(&root_node);
        let syms_with_patterns: Vec<Symbol> = reachable_ids
            .iter()
            .filter_map(|id| store.get_symbol_with_patterns(id).ok().flatten())
            .collect();
        let signals = analyze_symbols(&syms_with_patterns, &store);

        let analyze_out = render_analyze_output(ctx, &args.target, &root_node, &signals);
        emit(ctx, &analyze_out);
        return Ok(());
    }

    let out = match ctx.output {
        OutputFormat::Tree => {
            tree::render_tree(&[root_node], ctx.no_color, Some(ctx.depth as usize))
        }
        OutputFormat::Folded => folded::render_folded(&[root_node]),
        OutputFormat::Json => {
            let cochange = store.get_coupled(&sym.file.to_string_lossy(), 0.3, 5)?;
            let unresolved_edges = store.count_external_edges().unwrap_or(0);
            json::render_json(
                "paths",
                &args.target,
                ctx.depth,
                &format!("{:?}", ctx.weight).to_lowercase(),
                Some(root_node),
                &cochange,
                false,
                0,
                unresolved_edges,
            )
        }
        OutputFormat::Html => {
            let content = html::render_html(&[root_node], "paths", ctx.weight);
            let path = write_html(&ctx.repo_root, "paths", &content)?;
            eprintln!("{}", path);
            return Ok(());
        }
        OutputFormat::Tui => {
            return bs_render::tui::run_tui(
                &[root_node],
                &format!("paths {}", args.target),
                ctx.weight.describe(),
            );
        }
    };

    emit(ctx, &out);
    Ok(())
}

/// BFS through call edges to find the shortest path from `start` to `end`.
/// Returns None if no path exists within `max_depth` hops at `min_conf` path product.
fn find_path_to(
    store: &Store,
    start: &Symbol,
    end: &Symbol,
    max_depth: u32,
    min_conf: f32,
) -> Option<Vec<Symbol>> {
    if start.id == end.id {
        return Some(vec![start.clone()]);
    }

    let mut queue: VecDeque<(Symbol, Vec<Symbol>, f32)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    queue.push_back((start.clone(), vec![start.clone()], 1.0));
    visited.insert(start.id.clone());

    while let Some((current, path, path_conf)) = queue.pop_front() {
        if path.len() as u32 > max_depth {
            continue;
        }
        if let Ok(callees) = store.get_callees(&current.id, 0.0) {
            for (callee, conf) in callees {
                let child_conf = path_conf * conf;
                if child_conf < min_conf || visited.contains(&callee.id) {
                    continue;
                }
                let mut new_path = path.clone();
                new_path.push(callee.clone());
                if callee.id == end.id {
                    return Some(new_path);
                }
                visited.insert(callee.id.clone());
                queue.push_back((callee, new_path, child_conf));
            }
        }
    }

    None
}

/// Load patterns for a slice of symbols (patterns field is empty from get_callees).
fn enrich_with_patterns(store: &Store, syms: &[Symbol]) -> Vec<Symbol> {
    syms.iter()
        .map(|s| {
            store
                .get_symbol_with_patterns(&s.id)
                .ok()
                .flatten()
                .unwrap_or_else(|| s.clone())
        })
        .collect()
}

/// Build a linear TreeNode chain from a path (root → child → grandchild → ...).
fn build_path_tree(path: &[Symbol], max_churn: f32, max_loc: f32, weight: bs_render::Weight) -> TreeNode {
    let make_node = |sym: &Symbol| -> TreeNode {
        let mut n = TreeNode::leaf(
            sym.id.clone(),
            sym.name.clone(),
            sym.qualified.clone(),
            sym.file.to_string_lossy().into_owned(),
            sym.span,
        );
        n.weight = weight.score_symbol(sym, max_churn, max_loc);
        n
    };

    let mut nodes: Vec<TreeNode> = path.iter().map(make_node).collect();

    // Build from tail to head: last node has no children
    let mut child: Option<TreeNode> = None;
    for node in nodes.iter_mut().rev() {
        if let Some(c) = child.take() {
            node.children.push(c);
        }
        child = Some(node.clone());
    }

    child.unwrap_or_else(|| make_node(&path[0]))
}

/// Collect all symbol IDs in a tree (DFS).
fn collect_tree_ids(node: &TreeNode) -> Vec<String> {
    let mut ids = vec![node.id.clone()];
    for child in &node.children {
        ids.extend(collect_tree_ids(child));
    }
    ids
}

/// A single composable signal emitted for LLM consumption via `--analyze -o json`.
///
/// `kind`: `"path_depth"` | `"lock_await"` | `"blocking_async"` | `"high_complexity"` |
///         `"hot_symbol"` | `"cross_file_boundary"`
/// `severity`: `"info"` | `"medium"` | `"high"`
/// `detail`: human-readable explanation suitable for direct LLM prompt injection.
#[derive(Serialize)]
pub struct Signal {
    pub kind: String,
    pub severity: String,
    pub detail: String,
}

/// Analyze a list of symbols on the call path and return LLM-legible signals.
fn analyze_symbols(syms: &[Symbol], store: &Store) -> Vec<Signal> {
    let mut signals: Vec<Signal> = Vec::new();

    if syms.is_empty() {
        return signals;
    }

    // Path depth signal
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    signals.push(Signal {
        kind: "path_depth".into(),
        severity: if syms.len() > 5 { "medium" } else { "info" }.into(),
        detail: format!(
            "call chain has {} frame{}: {}",
            syms.len(),
            if syms.len() == 1 { "" } else { "s" },
            names.join(" → ")
        ),
    });

    // Cross-file boundary signal
    let files: Vec<String> = syms
        .iter()
        .map(|s| s.file.to_string_lossy().into_owned())
        .collect();
    let mut seen_files: Vec<String> = Vec::new();
    for f in &files {
        if !seen_files.contains(f) {
            seen_files.push(f.clone());
        }
    }
    if seen_files.len() > 1 {
        signals.push(Signal {
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

    // Per-symbol pattern and metric signals
    for sym in syms {
        let has_lock = sym.patterns.contains(&"lock".to_string());
        let has_await = sym.patterns.contains(&"await".to_string());
        let has_block_on = sym.patterns.contains(&"block_on".to_string());
        let has_loop = sym.patterns.contains(&"loop".to_string());
        let loc = sym.file.to_string_lossy();
        let at = format!("{}:{}", loc, sym.span.0);

        if has_lock && has_await {
            signals.push(Signal {
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
            signals.push(Signal {
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
            signals.push(Signal {
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
            signals.push(Signal {
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
            signals.push(Signal {
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

    // External boundary signal for the last symbol on the path
    if let Some(last) = syms.last() {
        if let Ok(externals) = store.get_external_callees(&last.id) {
            if !externals.is_empty() {
                signals.push(Signal {
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
    }

    signals
}

fn render_path_output(
    ctx: &Context,
    from: &str,
    to: &str,
    root_node: TreeNode,
    signals: Option<Vec<Signal>>,
    _store: &Store,
) -> Result<String> {
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
        _ => {
            let mut out = tree::render_tree(&[root_node], ctx.no_color, Some(ctx.depth as usize));
            if let Some(sigs) = signals {
                out.push_str(&render_signals_text(&sigs));
            }
            Ok(out)
        }
    }
}

fn render_analyze_output(
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
            let mut out = tree::render_tree(
                std::slice::from_ref(root_node),
                ctx.no_color,
                Some(ctx.depth as usize),
            );
            out.push_str(&render_signals_text(signals));
            out
        }
    }
}

fn render_signals_text(signals: &[Signal]) -> String {
    if signals.is_empty() {
        return "\n(no signals)\n".to_string();
    }
    let mut out = String::from("\nsignals:\n");
    for sig in signals {
        let sev = sig.severity.to_uppercase();
        let pad = "  ";
        out.push_str(&format!("\n  [{}]  {}\n", sev, sig.kind));
        // Word-wrap the detail at ~72 chars
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
    use bs_core::{model::SymbolKind, EdgeKind, LangId, Store};
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

    fn insert_sym(store: &Store, sym: &Symbol) {
        store
            .upsert_file(sym.file.to_str().unwrap(), &sym.lang, sym.loc)
            .unwrap();
        store.upsert_symbol(sym).unwrap();
    }

    fn insert_edge(store: &Store, from: &str, to: &str, conf: f32) {
        store.upsert_edge(from, to, &EdgeKind::Calls, conf, None).unwrap();
    }

    // --- find_path_to tests ---

    #[test]
    fn path_to_self_returns_single_node() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        insert_sym(&store, &a);
        let result = find_path_to(&store, &a, &a, 4, 0.0);
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn path_to_direct_callee() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_edge(&store, "a", "b", 1.0);

        let path = find_path_to(&store, &a, &b, 3, 0.0).unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].id, "a");
        assert_eq!(path[1].id, "b");
    }

    #[test]
    fn path_to_two_hops() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        let c = make_sym("c", "gamma", "src/c.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_sym(&store, &c);
        insert_edge(&store, "a", "b", 1.0);
        insert_edge(&store, "b", "c", 1.0);

        let path = find_path_to(&store, &a, &c, 4, 0.0).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[2].id, "c");
    }

    #[test]
    fn path_to_none_when_unreachable() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        // no edge
        assert!(find_path_to(&store, &a, &b, 4, 0.0).is_none());
    }

    #[test]
    fn path_to_none_when_depth_exceeded() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        let c = make_sym("c", "gamma", "src/c.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_sym(&store, &c);
        insert_edge(&store, "a", "b", 1.0);
        insert_edge(&store, "b", "c", 1.0);
        // max_depth=1 — path has 2 hops, unreachable
        assert!(find_path_to(&store, &a, &c, 1, 0.0).is_none());
    }

    #[test]
    fn path_to_pruned_by_confidence() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_edge(&store, "a", "b", 0.2); // low confidence edge
        // min_conf=0.5 prunes this edge
        assert!(find_path_to(&store, &a, &b, 4, 0.5).is_none());
    }

    // --- analyze_symbols tests ---

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
        assert!(sigs.iter().any(|s| s.kind == "blocking_async" && s.severity == "high"));
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
        assert!(sigs.iter().any(|s| s.kind == "hot_symbol" && s.severity == "medium"));
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

fn write_html(root: &std::path::Path, cmd: &str, content: &str) -> Result<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = format!("borescope-{}-{}.html", cmd, ts);
    let path = root.join(&filename);
    std::fs::write(&path, content)?;
    Ok(path.display().to_string())
}
