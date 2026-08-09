use super::{emit, open_store, Context};
use anyhow::Result;
use bs_render::{tree, OutputFormat, TreeNode, Weight};
use clap::Args;
use std::collections::HashMap;

#[derive(Args)]
pub struct MapArgs {}

pub fn run(ctx: &Context, _args: &MapArgs) -> Result<()> {
    let store = open_store(ctx)?;

    let out = if ctx.zoom == "fn" || ctx.zoom == "mod" {
        // Symbol-level: group symbols by file
        let all_syms = store.all_symbols()?;
        let max_churn = all_syms
            .iter()
            .map(|s| s.churn as f32)
            .fold(0.0f32, f32::max);
        let max_loc = all_syms.iter().map(|s| s.loc as f32).fold(0.0f32, f32::max);

        // Group by file
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

        match ctx.output {
            OutputFormat::Tree | OutputFormat::Folded => {
                tree::render_tree(&nodes, ctx.weight, ctx.no_color, Some(ctx.depth as usize))
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
        }
    } else {
        // File/package level
        let stats = match ctx.weight {
            Weight::Churn | Weight::Hotspot => store.get_map_by_churn()?,
            _ => store.get_all_file_stats()?,
        };
        match ctx.output {
            OutputFormat::Tree | OutputFormat::Folded => {
                tree::render_file_tree(&stats, ctx.weight, ctx.no_color)
            }
            OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
            OutputFormat::Html | OutputFormat::Tui => {
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
                if ctx.output == OutputFormat::Tui {
                    return bs_render::tui::run_tui(&nodes, "map", ctx.weight.describe());
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
