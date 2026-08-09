use super::{emit, open_store, Context};
use anyhow::Result;
use bs_git::Miner;
use bs_render::{folded, html, json, tree, OutputFormat, TreeNode};
use clap::Args;

#[derive(Args)]
pub struct BranchArgs {
    /// Branch name
    pub name: String,

    /// Base revision (default: main or master)
    #[arg(long)]
    pub base: Option<String>,
}

pub fn run(ctx: &Context, args: &BranchArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let miner = Miner::new(ctx.repo_root.clone());

    let base = args.base.as_deref().unwrap_or("main");

    let merge_base = miner.merge_base(base, &args.name)?;
    let changed_files = miner.changed_files(&merge_base, &args.name)?;

    let mut nodes = Vec::new();
    for file in &changed_files {
        let syms = store.symbols_for_file(file)?;
        for sym in syms {
            let mut node = TreeNode::leaf(
                sym.id.clone(),
                sym.name.clone(),
                sym.qualified.clone(),
                sym.file.to_string_lossy().into_owned(),
                sym.span,
            );
            node.mark = Some("~".to_string());
            nodes.push(node);
        }
    }

    let out = match ctx.output {
        OutputFormat::Tree => {
            tree::render_tree(&nodes, ctx.weight, ctx.no_color, Some(ctx.depth as usize))
        }
        OutputFormat::Folded => folded::render_folded(&nodes),
        OutputFormat::Json => json::render_json(
            "branch",
            &args.name,
            ctx.depth,
            &format!("{:?}", ctx.weight).to_lowercase(),
            nodes.into_iter().next(),
            &[],
            false,
            0,
        ),
        OutputFormat::Html => {
            let content = html::render_html(&nodes, &format!("branch:{}", args.name), ctx.weight);
            let path = write_html(&ctx.repo_root, "branch", &content)?;
            eprintln!("{}", path);
            return Ok(());
        }
        OutputFormat::Tui => {
            return bs_render::tui::run_tui(
                &nodes,
                &format!("branch {}", args.name),
                ctx.weight.describe(),
            );
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
