mod bfs;
mod signals;

use super::{build_tree_node, emit, open_store, resolve_target, Context};
use anyhow::Result;
use bs_render::{self, folded, html, json, tree, OutputFormat};
use clap::Args;

use bfs::{build_path_tree, collect_tree_ids, enrich_with_patterns, find_path_to};
use signals::{analyze_symbols, render_analyze_output, render_path_output};

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
    let max_churn = all_syms
        .iter()
        .map(|s| s.churn as f32)
        .fold(0.0f32, f32::max);
    let max_loc = all_syms.iter().map(|s| s.loc as f32).fold(0.0f32, f32::max);

    // --to mode — BFS to find shortest path to a target symbol
    if let Some(ref to_target) = args.to {
        let end_sym = resolve_target(&store, to_target)?;
        let path = find_path_to(&store, &sym, &end_sym, ctx.depth, ctx.min_confidence);

        let (root_node, path_syms) = if let Some(path) = path {
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

        let out = render_path_output(ctx, &args.target, to_target, root_node, signals)?;
        emit(ctx, &out);
        return Ok(());
    }

    // Normal mode — full call tree from start symbol
    let mut visited = std::collections::HashSet::new();
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
        let reachable_ids: Vec<String> = collect_tree_ids(&root_node);
        let syms_with_patterns: Vec<_> = reachable_ids
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
        OutputFormat::Mermaid => {
            bs_render::mermaid::render_flowchart(&[root_node], "TD", ctx.no_fence)
        }
        OutputFormat::Dot => bs_render::dot::render_flowchart(&[root_node], ctx.no_fence),
    };

    emit(ctx, &out);
    Ok(())
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
