use super::{build_tree_node, emit, open_store, resolve_target, Context};
use anyhow::Result;
use bs_render::{folded, html, json, tree, OutputFormat};
use clap::Args;
use std::collections::HashSet;

#[derive(Args)]
pub struct PathsArgs {
    /// Target: path/to/file.go:FuncName | path/to/file.go:42 | QualifiedName
    pub target: String,
}

pub fn run(ctx: &Context, args: &PathsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let sym = resolve_target(&store, &args.target)?;

    let all_syms = store.all_symbols()?;
    let max_churn = all_syms.iter().map(|s| s.churn as f32).fold(0.0f32, f32::max);

    let mut visited = HashSet::new();
    let root_node = build_tree_node(
        &store,
        &sym,
        0,
        ctx.depth,
        ctx.min_confidence,
        ctx.weight,
        max_churn,
        &mut visited,
    );

    let out = match ctx.output {
        OutputFormat::Tree => {
            tree::render_tree(&[root_node], ctx.weight, ctx.no_color, Some(ctx.depth as usize))
        }
        OutputFormat::Folded => folded::render_folded(&[root_node]),
        OutputFormat::Json => {
            let cochange = store.get_coupled(&sym.file.to_string_lossy(), 0.3, 5)?;
            json::render_json(
                "paths",
                &args.target,
                ctx.depth,
                &format!("{:?}", ctx.weight).to_lowercase(),
                Some(root_node),
                &cochange,
                false,
                0,
            )
        }
        OutputFormat::Html => {
            let content = html::render_html(&[root_node], "paths", ctx.weight);
            let path = write_html(&ctx.repo_root, "paths", &content)?;
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
