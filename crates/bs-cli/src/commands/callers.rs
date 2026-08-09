use super::{build_callers_tree_node, emit, open_store, resolve_target, Context};
use anyhow::Result;
use bs_render::{self, folded, html, json, tree, OutputFormat};
use clap::Args;
use std::collections::HashSet;

#[derive(Args)]
pub struct CallersArgs {
    /// Target symbol
    pub target: String,

    /// Append co-change section
    #[arg(long, default_value = "false")]
    pub coupled: bool,
}

pub fn run(ctx: &Context, args: &CallersArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let sym = resolve_target(&store, &args.target)?;

    let all_syms = store.all_symbols()?;
    let max_churn = all_syms.iter().map(|s| s.churn as f32).fold(0.0f32, f32::max);

    let mut visited = HashSet::new();
    let root_node = build_callers_tree_node(
        &store,
        &sym,
        0,
        ctx.depth,
        ctx.min_confidence,
        ctx.weight,
        max_churn,
        &mut visited,
    );

    let cochange = if args.coupled || matches!(ctx.output, OutputFormat::Json) {
        store.get_coupled(&sym.file.to_string_lossy(), 0.3, 5)?
    } else {
        vec![]
    };

    let out = match ctx.output {
        OutputFormat::Tree => {
            let mut out =
                tree::render_tree(&[root_node], ctx.weight, ctx.no_color, Some(ctx.depth as usize));
            if args.coupled && !cochange.is_empty() {
                out.push_str("\nCo-changed files:\n");
                for c in &cochange {
                    out.push_str(&format!(
                        "  {} ← strength {:.2} (support {})\n",
                        c.file_b, c.strength, c.support
                    ));
                }
            }
            out
        }
        OutputFormat::Folded => folded::render_folded(&[root_node]),
        OutputFormat::Json => json::render_json(
            "callers",
            &args.target,
            ctx.depth,
            &format!("{:?}", ctx.weight).to_lowercase(),
            Some(root_node),
            &cochange,
            false,
            0,
        ),
        OutputFormat::Html => {
            let content = html::render_html(&[root_node], "callers", ctx.weight);
            let path = write_html(&ctx.repo_root, "callers", &content)?;
            eprintln!("{}", path);
            return Ok(());
        }
        OutputFormat::Tui => {
            return bs_render::tui::run_tui(&[root_node], &format!("callers {}", args.target), ctx.weight.describe());
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
