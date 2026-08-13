use super::{emit, open_store, Context};
use anyhow::Result;
use bs_render::{tree, OutputFormat, TreeNode, Weight};
use clap::Args;
use std::collections::HashMap;

#[derive(Args)]
pub struct MapArgs {
    /// Limit output to top N files by weight (default: 50)
    #[arg(long, default_value = "50")]
    pub top: usize,
}

pub fn run(ctx: &Context, args: &MapArgs) -> Result<()> {
    let store = open_store(ctx)?;

    let out = if ctx.zoom == "fn" || ctx.zoom == "mod" {
        let all_syms = store.all_symbols()?;
        let max_churn = all_syms
            .iter()
            .map(|s| s.churn as f32)
            .fold(0.0f32, f32::max);
        let max_loc = all_syms.iter().map(|s| s.loc as f32).fold(0.0f32, f32::max);

        let mut by_file: HashMap<String, Vec<&bs_core::Symbol>> = HashMap::new();
        for sym in &all_syms {
            by_file
                .entry(sym.file.to_string_lossy().into_owned())
                .or_default()
                .push(sym);
        }

        let mut file_keys: Vec<String> = by_file.keys().cloned().collect();
        file_keys.sort();

        let nodes: Vec<TreeNode> = file_keys
            .iter()
            .map(|file| {
                let syms = &by_file[file];
                let children: Vec<TreeNode> = syms
                    .iter()
                    .map(|sym| {
                        let mut n = TreeNode::leaf(
                            sym.id.clone(),
                            sym.name.clone(),
                            sym.qualified.clone(),
                            file.clone(),
                            sym.span,
                        );
                        n.weight = ctx.weight.score_symbol(sym, max_churn, max_loc);
                        n
                    })
                    .collect();
                let max_child_weight = children.iter().map(|c| c.weight).fold(0.0f32, f32::max);
                let mut file_node = TreeNode::leaf(
                    format!("file:{}", file),
                    file.clone(),
                    file.clone(),
                    file.clone(),
                    (0, 0),
                );
                file_node.weight = max_child_weight;
                file_node.children = children;
                file_node
            })
            .collect();

        // Apply --top N: sort by weight desc, truncate, append footer if omitted
        let total = nodes.len();
        let mut nodes = nodes;
        nodes.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        let omitted = total.saturating_sub(args.top);
        nodes.truncate(args.top);

        match ctx.output {
            OutputFormat::Tree | OutputFormat::Folded => {
                let mut out = tree::render_tree(&nodes, ctx.no_color, Some(ctx.depth as usize));
                if omitted > 0 {
                    out.push_str(&format!(
                        "\n(+{} files not shown — use --top {} to see more)\n",
                        omitted, total
                    ));
                }
                out
            }
            OutputFormat::Json => serde_json::to_string_pretty(&nodes).unwrap_or_default(),
            OutputFormat::Html => {
                let content = bs_render::html::render_html(&nodes, "map", ctx.weight);
                let path = write_html(&ctx.repo_root, "map", &content)?;
                eprintln!("{}", path);
                return Ok(());
            }
            OutputFormat::Tui => {
                return bs_render::tui::run_tui(&nodes, "map", ctx.weight.describe());
            }
            OutputFormat::Mermaid => {
                bs_render::mermaid::render_flowchart(&nodes, "TD", ctx.no_fence)
            }
            OutputFormat::Dot => bs_render::dot::render_flowchart(&nodes, ctx.no_fence),
        }
    } else {
        let stats = match ctx.weight {
            Weight::Churn | Weight::Hotspot => store.get_map_by_churn()?,
            _ => store.get_all_file_stats()?,
        };
        match ctx.output {
            OutputFormat::Tree | OutputFormat::Folded => tree::render_file_tree(&stats, ctx.weight),
            OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
            OutputFormat::Html | OutputFormat::Tui | OutputFormat::Mermaid | OutputFormat::Dot => {
                let max_c = stats.iter().map(|x| x.churn as f32).fold(0.0f32, f32::max);
                let max_l = stats.iter().map(|x| x.loc as f32).fold(0.0f32, f32::max);
                let nodes: Vec<TreeNode> = stats
                    .iter()
                    .map(|s| {
                        let mut n = TreeNode::leaf(
                            format!("file:{}", s.path),
                            s.path.clone(),
                            s.path.clone(),
                            s.path.clone(),
                            (0, s.loc),
                        );
                        n.weight = ctx.weight.score_file(s, max_c, max_l);
                        n
                    })
                    .collect();
                match ctx.output {
                    OutputFormat::Tui => {
                        return bs_render::tui::run_tui(&nodes, "map", ctx.weight.describe());
                    }
                    OutputFormat::Mermaid => {
                        emit(ctx, &bs_render::mermaid::render_flowchart(&nodes, "TD", ctx.no_fence));
                        return Ok(());
                    }
                    OutputFormat::Dot => {
                        emit(ctx, &bs_render::dot::render_flowchart(&nodes, ctx.no_fence));
                        return Ok(());
                    }
                    _ => {}
                }
                let content = bs_render::html::render_html(&nodes, "map", ctx.weight);
                let path = write_html(&ctx.repo_root, "map", &content)?;
                eprintln!("{}", path);
                return Ok(());
            }
        }
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
