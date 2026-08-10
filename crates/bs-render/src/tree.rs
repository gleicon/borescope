use crate::{TreeNode, Weight};
use bs_core::FileStat;

pub fn render_tree(nodes: &[TreeNode], no_color: bool, collapse_depth: Option<usize>) -> String {
    let mut out = String::new();
    for node in nodes {
        render_node(&mut out, node, "", true, true, no_color, collapse_depth, 0);
    }
    out
}

pub fn render_file_tree(stats: &[FileStat], weight: Weight) -> String {
    let max_churn = stats.iter().map(|s| s.churn as f32).fold(0.0f32, f32::max);
    let max_loc = stats.iter().map(|s| s.loc as f32).fold(0.0f32, f32::max);

    let mut out = String::new();
    for (i, stat) in stats.iter().enumerate() {
        let w = weight.score_file(stat, max_churn, max_loc);
        let is_last = i == stats.len() - 1;
        let prefix = if is_last { "└─ " } else { "├─ " };
        let bar = if !matches!(weight, Weight::None) {
            format!("  {}", weight_bar(w))
        } else {
            String::new()
        };
        out.push_str(&format!("{}{}{}\n", prefix, stat.path, bar));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    out: &mut String,
    node: &TreeNode,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    no_color: bool,
    collapse_depth: Option<usize>,
    depth: usize,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };

    let ext_annotation = if node.external { " (ext)" } else { "" };
    let conf_annotation = if !node.external && node.confidence < 0.7 {
        format!(" ┄┄ {:.1}", node.confidence)
    } else {
        String::new()
    };

    let mark_str = match node.mark.as_deref() {
        Some("+") => {
            if no_color {
                "+ ".to_string()
            } else {
                "\x1b[32m+\x1b[0m ".to_string()
            }
        }
        Some("-") => {
            if no_color {
                "- ".to_string()
            } else {
                "\x1b[31m-\x1b[0m ".to_string()
            }
        }
        Some("~") => {
            if no_color {
                "~ ".to_string()
            } else {
                "\x1b[33m~\x1b[0m ".to_string()
            }
        }
        _ => String::new(),
    };

    let bar = if node.weight > 0.0 {
        format!("  {}", weight_bar(node.weight))
    } else {
        String::new()
    };

    out.push_str(&format!(
        "{}{}{}{}{}{}{}\n",
        prefix, connector, mark_str, node.name, ext_annotation, conf_annotation, bar
    ));

    if node.children.is_empty() {
        return;
    }

    if let Some(max_depth) = collapse_depth {
        if depth + 1 >= max_depth {
            let child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
            out.push_str(&format!(
                "{}└─ ▸ ({} more)\n",
                child_prefix,
                node.children.len()
            ));
            return;
        }
    }

    let child_prefix = format!(
        "{}{}",
        prefix,
        if is_root || is_last { "   " } else { "│  " }
    );

    for (i, child) in node.children.iter().enumerate() {
        let last = i == node.children.len() - 1;
        render_node(
            out,
            child,
            &child_prefix,
            last,
            false,
            no_color,
            collapse_depth,
            depth + 1,
        );
    }
}

fn weight_bar(w: f32) -> String {
    let filled = (w * 6.0).round() as usize;
    let bar: String = "█".repeat(filled.min(6));
    let empty: String = " ".repeat(6 - filled.min(6));
    format!("{}{} {:.2}", bar, empty, w)
}
