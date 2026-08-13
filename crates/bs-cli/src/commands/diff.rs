use super::{emit, open_store, Context};
use anyhow::Result;
use bs_core::Store;
use bs_git::{FileDiffRanges, Miner};
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

    // Always compute line ranges — needed for +/~ polarity and --weight diff scoring
    let diff_ranges = miner.diff_line_ranges_full(rev1, rev2)?;

    let nodes = build_diff_nodes(&store, &changed_files, &diff_ranges, ctx.weight, ctx.depth)?;

    let target = format!("{}..{}", rev1, rev2.unwrap_or("worktree"));
    let out = match ctx.output {
        OutputFormat::Tree => tree::render_tree(&nodes, ctx.no_color, Some(ctx.depth as usize)),
        OutputFormat::Json => json::render_json(
            "diff",
            &target,
            ctx.depth,
            &format!("{:?}", ctx.weight).to_lowercase(),
            nodes.into_iter().next(),
            &[],
            false,
            0,
            0,
        ),
        OutputFormat::Folded => bs_render::folded::render_folded(&nodes),
        OutputFormat::Html => {
            let content = html::render_html(&nodes, "diff", ctx.weight);
            let path = write_html(&ctx.repo_root, "diff", &content)?;
            eprintln!("{}", path);
            return Ok(());
        }
        OutputFormat::Tui => {
            return bs_render::tui::run_tui(&nodes, "diff", ctx.weight.describe());
        }
        OutputFormat::Mermaid => {
            bs_render::mermaid::render_flowchart(&nodes, "TD", ctx.no_fence)
        }
        OutputFormat::Dot => bs_render::dot::render_flowchart(&nodes, ctx.no_fence),
    };

    emit(ctx, &out);
    Ok(())
}

fn build_diff_nodes(
    store: &Store,
    changed_files: &[String],
    diff_ranges: &HashMap<String, FileDiffRanges>,
    weight: Weight,
    _depth: u32,
) -> Result<Vec<TreeNode>> {
    let mut nodes = Vec::new();

    // Normalize --weight diff by total new-file lines touched across all files
    let total_diff_lines: u32 = diff_ranges
        .values()
        .map(|r| r.all_touched.len() as u32)
        .sum();

    for file in changed_files {
        let syms = store.symbols_for_file(file)?;
        for sym in syms {
            let file_ranges = diff_ranges.get(file.as_str());
            let all_touched = file_ranges.map(|r| &r.all_touched);
            let pure_added = file_ranges.map(|r| &r.pure_added);

            let span_all = sym_lines_touched(&sym.span, all_touched);
            let span_pure = sym_lines_touched(&sym.span, pure_added);

            let w = if matches!(weight, Weight::Diff) {
                span_all as f32 / total_diff_lines.max(1) as f32
            } else {
                0.0
            };

            // Classify hunk polarity per symbol span
            let mark = if span_all == 0 {
                None // span not touched by any hunk
            } else if span_pure == span_all {
                Some("+".to_string()) // every touched line is a pure addition → new code
            } else {
                Some("~".to_string()) // at least one line modified existing code
            };

            let mut node = TreeNode::leaf(
                sym.id.clone(),
                sym.name.clone(),
                sym.qualified.clone(),
                sym.file.to_string_lossy().into_owned(),
                sym.span,
            );
            node.mark = mark;
            node.weight = w;
            nodes.push(node);
        }
    }

    Ok(nodes)
}

fn sym_lines_touched(span: &(u32, u32), touched: Option<&HashSet<u32>>) -> u32 {
    match touched {
        None => 0,
        Some(set) => (span.0..=span.1).filter(|l| set.contains(l)).count() as u32,
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

#[cfg(test)]
mod tests {
    use bs_git::parse_diff_ranges;

    const SAMPLE_DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index abc..def 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,4 @@ fn foo() {
+    let x = 1;
+    let y = 2;
@@ -20,1 +21,0 @@
-    old_line
diff --git a/src/main.rs b/src/main.rs
index ghi..jkl 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,3 @@
+    new_line
"#;

    #[test]
    fn test_parse_diff_ranges() {
        let ranges = parse_diff_ranges(SAMPLE_DIFF);
        let lib = ranges.get("src/lib.rs").expect("src/lib.rs in diff");
        assert!(lib.contains(&10), "line 10 touched");
        assert!(lib.contains(&11), "line 11 touched");
        let main = ranges.get("src/main.rs").expect("src/main.rs in diff");
        assert!(main.contains(&1), "line 1 touched in main.rs");
    }
}
