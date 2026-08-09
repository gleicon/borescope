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

    // For --weight diff: get per-file per-line touched set
    let line_ranges = if matches!(ctx.weight, Weight::Diff) {
        miner.diff_line_ranges(rev1, rev2)?
    } else {
        HashMap::new()
    };

    let nodes = build_diff_nodes(&store, &changed_files, &line_ranges, ctx.weight, ctx.depth)?;

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
    };

    emit(ctx, &out);
    Ok(())
}

fn build_diff_nodes(
    store: &Store,
    changed_files: &[String],
    line_ranges: &HashMap<String, HashSet<u32>>,
    weight: Weight,
    _depth: u32,
) -> Result<Vec<TreeNode>> {
    let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let mut nodes = Vec::new();

    // Compute total touched lines across all files for normalization
    let total_diff_lines: u32 = line_ranges.values().map(|s| s.len() as u32).sum();

    for file in changed_files {
        let syms = store.symbols_for_file(file)?;
        for sym in syms {
            let file_touched = line_ranges.get(file.as_str());
            let span_touched = sym_lines_touched(&sym.span, file_touched);
            let w = if matches!(weight, Weight::Diff) {
                span_touched as f32 / total_diff_lines.max(1) as f32
            } else {
                0.0
            };

            let mark = if changed_set.contains(sym.file.to_str().unwrap_or("")) {
                if span_touched > 0 {
                    Some("~".to_string())
                } else {
                    None
                }
            } else {
                None
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
