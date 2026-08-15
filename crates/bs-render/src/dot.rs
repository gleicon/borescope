//! Graphviz DOT diagram renderers.
//!
//! Each function emits a fenced ```dot``` block by default.
//! Pass `no_fence = true` for raw DOT syntax (piping to `dot -Tpng`, etc.).
//! DOT is preferred over Mermaid for large graphs where layout quality matters.

use crate::TreeNode;
use bs_core::CoChange;
use std::collections::HashMap;

fn fence(content: &str, no_fence: bool) -> String {
    if no_fence {
        content.to_string()
    } else {
        format!("```dot\n{}```\n", content)
    }
}

fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Digraph from a linear call chain (`paths --to` output).
pub fn render_sequence(root: &TreeNode, no_fence: bool) -> String {
    let mut chain: Vec<&TreeNode> = Vec::new();
    let mut cur = root;
    loop {
        chain.push(cur);
        match cur.children.first() {
            Some(child) => cur = child,
            None => break,
        }
    }

    let mut out = String::from(
        "digraph borescope_path {\n    rankdir=LR;\n    node [shape=box fontname=monospace];\n",
    );

    for node in &chain {
        let label = dot_escape(&node.qualified);
        let weight_attr = if node.weight > 0.0 {
            format!(
                " fillcolor=\"/reds/9/{:.0}\" style=filled",
                (node.weight * 8.0).ceil().max(1.0)
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "    \"{}\" [label=\"{}\"{weight_attr}];\n",
            label, label
        ));
    }

    for i in 0..chain.len().saturating_sub(1) {
        let from = dot_escape(&chain[i].qualified);
        let to = dot_escape(&chain[i + 1].qualified);
        let conf = chain[i + 1].confidence;
        let label = if conf < 1.0 {
            format!("{:.0}%", conf * 100.0)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
            from, to, label
        ));
    }

    out.push_str("}\n");
    fence(&out, no_fence)
}

/// Digraph from a call tree (`paths`, `map`, `callers`).
pub fn render_flowchart(nodes: &[TreeNode], no_fence: bool) -> String {
    let mut out = String::from(
        "digraph borescope_map {\n    rankdir=TD;\n    node [shape=box fontname=monospace];\n",
    );
    let mut edge_lines: Vec<String> = Vec::new();
    render_dot_nodes(nodes, &mut out, &mut edge_lines);
    for e in edge_lines {
        out.push_str(&e);
    }
    out.push_str("}\n");
    fence(&out, no_fence)
}

fn render_dot_nodes(nodes: &[TreeNode], out: &mut String, edges: &mut Vec<String>) {
    for node in nodes {
        let label = dot_escape(&node.name);
        let qualified = dot_escape(&node.qualified);
        let attrs = if node.external {
            " style=dashed color=gray".to_string()
        } else if node.weight > 0.0 {
            format!(
                " fillcolor=\"/reds/9/{:.0}\" style=filled",
                (node.weight * 8.0).ceil().max(1.0)
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "    \"{}\" [label=\"{}\"{attrs}];\n",
            qualified, label
        ));

        for child in &node.children {
            let child_q = dot_escape(&child.qualified);
            let conf_label = if child.confidence < 1.0 {
                format!("{:.0}%", child.confidence * 100.0)
            } else {
                String::new()
            };
            edges.push(format!(
                "    \"{}\" -> \"{}\" [label=\"{}\"];\n",
                qualified, child_q, conf_label
            ));
        }

        if !node.children.is_empty() {
            render_dot_nodes(&node.children, out, edges);
        }
    }
}

/// Undirected graph from co-change data (`coupled` output).
pub fn render_dependency(cochange: &[CoChange], target: &str, no_fence: bool) -> String {
    let mut out = String::from(
        "graph borescope_coupled {\n    rankdir=LR;\n    node [shape=box fontname=monospace];\n",
    );
    let te = dot_escape(target);
    out.push_str(&format!(
        "    \"{}\" [style=filled fillcolor=orange];\n",
        te
    ));

    for c in cochange {
        let partner = if c.file_a == target {
            &c.file_b
        } else {
            &c.file_a
        };
        let pe = dot_escape(partner);
        out.push_str(&format!("    \"{}\" [shape=box];\n", pe));
        out.push_str(&format!(
            "    \"{}\" -- \"{}\" [label=\"{:.2} ({} commits)\"];\n",
            te, pe, c.strength, c.support
        ));
    }

    out.push_str("}\n");
    fence(&out, no_fence)
}

/// Class-like digraph from smell findings. Files are cluster subgraphs;
/// smell kinds are nodes within them.
pub fn render_class(findings: &[(String, String)], no_fence: bool) -> String {
    let mut by_file: HashMap<String, Vec<String>> = HashMap::new();
    for (file, kind) in findings {
        by_file.entry(file.clone()).or_default().push(kind.clone());
    }

    let mut out = String::from(
        "digraph borescope_smells {\n    rankdir=LR;\n    node [shape=plaintext fontname=monospace];\n    compound=true;\n",
    );

    let mut files: Vec<String> = by_file.keys().cloned().collect();
    files.sort();

    for (i, file) in files.iter().enumerate() {
        let fe = dot_escape(file);
        let kinds = &by_file[file];
        out.push_str(&format!("    subgraph cluster_{} {{\n", i));
        out.push_str(&format!("        label=\"{}\";\n", fe));
        out.push_str("        style=rounded;\n");
        for kind in kinds.iter().take(6) {
            let ke = dot_escape(kind);
            out.push_str(&format!("        \"{}:{}\" [label=\"{}\"];\n", fe, ke, ke));
        }
        if kinds.len() > 6 {
            out.push_str(&format!(
                "        \"{}:more\" [label=\"…{} more\" style=dashed];\n",
                fe,
                kinds.len() - 6
            ));
        }
        out.push_str("    }\n");
    }

    out.push_str("}\n");
    fence(&out, no_fence)
}
