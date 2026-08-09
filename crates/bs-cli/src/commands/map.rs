use super::{emit, open_store, Context};
use anyhow::Result;
use bs_render::{tree, OutputFormat, Weight};
use clap::Args;

#[derive(Args)]
pub struct MapArgs {}

pub fn run(ctx: &Context, _args: &MapArgs) -> Result<()> {
    let store = open_store(ctx)?;

    let stats = match ctx.weight {
        Weight::Churn | Weight::Hotspot => store.get_map_by_churn()?,
        _ => store.get_all_file_stats()?,
    };

    let out = match ctx.output {
        OutputFormat::Tree | OutputFormat::Folded => {
            tree::render_file_tree(&stats, ctx.weight, ctx.no_color)
        }
        OutputFormat::Json => {
            serde_json::to_string_pretty(&stats).unwrap_or_default()
        }
        OutputFormat::Html => {
            // Convert to tree nodes for HTML render
            let nodes: Vec<bs_render::TreeNode> = stats
                .iter()
                .map(|s| {
                    let mut n = bs_render::TreeNode::leaf(
                        format!("file:{}", s.path),
                        s.path.clone(),
                        s.path.clone(),
                        s.path.clone(),
                        (0, s.loc),
                    );
                    n.weight = ctx.weight.score_file(s, 1.0, 1.0);
                    n
                })
                .collect();
            let content = bs_render::html::render_html(&nodes, "map", ctx.weight);
            let path = write_html(&ctx.repo_root, "map", &content)?;
            eprintln!("{}", path);
            return Ok(());
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
