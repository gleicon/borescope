use super::{emit, open_store, Context};
use anyhow::Result;
use bs_core::Store;
use bs_git::Miner;
use bs_render::{html, json, tree, OutputFormat, TreeNode, Weight};
use clap::Args;
use std::collections::{HashMap, HashSet};

#[derive(Args)]
pub struct DiffArgs {
    /// First revision (default: HEAD)
    pub rev1: Option<String>,
    /// Second revision (default: worktree)
    pub rev2: Option<String>,
}

pub fn run(ctx: &Context, args: &DiffArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let miner = Miner::new(ctx.repo_root.clone());

    let rev1 = args.rev1.as_deref().unwrap_or("HEAD");
    let rev2 = args.rev2.as_deref();

    let changed_files = match rev2 {
        Some(r2) => miner.changed_files(rev1, r2)?,
        None => miner.changed_files_worktree(rev1)?,
    };

    let nodes = build_diff_nodes(&store, &changed_files, ctx.weight, ctx.depth)?;

    let out = match ctx.output {
        OutputFormat::Tree => {
            tree::render_tree(&nodes, ctx.weight, ctx.no_color, Some(ctx.depth as usize))
        }
        OutputFormat::Json => json::render_json(
            "diff",
            &format!("{}..{}", rev1, rev2.unwrap_or("worktree")),
            ctx.depth,
            &format!("{:?}", ctx.weight).to_lowercase(),
            nodes.into_iter().next(),
            &[],
            false,
            0,
        ),
        OutputFormat::Folded => bs_render::folded::render_folded(&nodes),
        OutputFormat::Html => {
            let content = html::render_html(&nodes, "diff", ctx.weight);
            let path = write_html(&ctx.repo_root, "diff", &content)?;
            eprintln!("{}", path);
            return Ok(());
        }
    };

    emit(ctx, &out);
    Ok(())
}

fn build_diff_nodes(
    store: &Store,
    changed_files: &[String],
    weight: Weight,
    depth: u32,
) -> Result<Vec<TreeNode>> {
    let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let mut nodes = Vec::new();

    for file in changed_files {
        let syms = store.symbols_for_file(file)?;
        for sym in syms {
            let is_changed = changed_set.contains(sym.file.to_str().unwrap_or(""));
            let mark = if is_changed { Some("~".to_string()) } else { None };

            let mut node = TreeNode::leaf(
                sym.id.clone(),
                sym.name.clone(),
                sym.qualified.clone(),
                sym.file.to_string_lossy().into_owned(),
                sym.span,
            );
            node.mark = mark;
            nodes.push(node);
        }
    }

    Ok(nodes)
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
