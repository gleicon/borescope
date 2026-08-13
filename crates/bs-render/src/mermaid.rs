//! Mermaid diagram renderers.
//!
//! Each function emits a fenced ```mermaid``` block by default.
//! Pass `no_fence = true` to emit raw Mermaid syntax (useful when piping
//! to an agent or script that wraps the block itself).

use crate::TreeNode;
use bs_core::CoChange;
use std::collections::HashMap;

fn fence(content: &str, no_fence: bool) -> String {
    if no_fence {
        content.to_string()
    } else {
        format!("```mermaid\n{}```\n", content)
    }
}

fn safe_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Sequence diagram from a linear call chain (`paths --to` output).
/// Walks root→first-child→grandchild. If the chain branches, only the
/// first branch at each level is followed.
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

    let mut out = String::from("sequenceDiagram\n");
    let ids: Vec<String> = chain
        .iter()
        .map(|n| {
            format!(
                "P{}",
                safe_id(&n.id).chars().take(24).collect::<String>()
            )
        })
        .collect();

    for (i, node) in chain.iter().enumerate() {
        out.push_str(&format!(
            "    participant {} as {}\n",
            ids[i], node.name
        ));
    }

    for i in 0..chain.len().saturating_sub(1) {
        let conf = chain[i + 1].confidence;
        let label = if conf < 1.0 {
            format!("calls [{:.0}%]", conf * 100.0)
        } else {
            "calls".to_string()
        };
        out.push_str(&format!(
            "    {}->>+{}: {}\n",
            ids[i], ids[i + 1], label
        ));
    }

    fence(&out, no_fence)
}

/// Flowchart from a call tree (`paths`, `map`, `callers`).
///
/// `direction`: `"TD"` (top-down, forward slice), `"BT"` (bottom-to-top, callers tree).
pub fn render_flowchart(nodes: &[TreeNode], direction: &str, no_fence: bool) -> String {
    let mut out = format!("flowchart {}\n", direction);
    let mut edge_lines: Vec<String> = Vec::new();
    render_flowchart_nodes(nodes, &mut out, &mut edge_lines, 0);
    for e in edge_lines {
        out.push_str(&e);
    }
    fence(&out, no_fence)
}

fn render_flowchart_nodes(
    nodes: &[TreeNode],
    out: &mut String,
    edges: &mut Vec<String>,
    depth: usize,
) {
    let indent = "    ".repeat(depth + 1);
    for node in nodes {
        let nid = format!("N{}", safe_id(&node.id).chars().take(30).collect::<String>());
        let label = if node.weight > 0.0 {
            format!("{}\\nw={:.2}", node.name, node.weight)
        } else {
            node.name.clone()
        };
        let shape = if node.external { "(({}))" } else { "[\"{}\"]" };
        out.push_str(&format!("{}{}{};\n", indent, nid, shape.replace("{}", &label)));

        for child in &node.children {
            let cid =
                format!("N{}", safe_id(&child.id).chars().take(30).collect::<String>());
            edges.push(format!(
                "{}{}-->{}\n",
                indent, nid, cid
            ));
        }

        if !node.children.is_empty() {
            render_flowchart_nodes(&node.children, out, edges, depth + 1);
        }
    }
}

/// Dependency graph from co-change data (`coupled` output).
pub fn render_dependency(cochange: &[CoChange], target: &str, no_fence: bool) -> String {
    let mut out = String::from("flowchart LR\n");
    let tid = format!("T{}", safe_id(target).chars().take(30).collect::<String>());
    out.push_str(&format!("    {}[\"{}\"]:::target\n", tid, target));

    for c in cochange {
        let partner = if c.file_a == target {
            &c.file_b
        } else {
            &c.file_a
        };
        let pid = format!("P{}", safe_id(partner).chars().take(30).collect::<String>());
        out.push_str(&format!("    {}[\"{}\"]\n", pid, partner));
        out.push_str(&format!(
            "    {} <-->|\"{:.2} ({} commits)\"| {}\n",
            tid, c.strength, c.support, pid
        ));
    }

    out.push_str("    classDef target fill:#f96,stroke:#333,color:#000\n");
    fence(&out, no_fence)
}

/// Class diagram from smell findings.
///
/// `findings`: `(file, kind)` pairs — any number per file. Files become
/// classes; smell kinds become members.
pub fn render_class(findings: &[(String, String)], no_fence: bool) -> String {
    let mut by_file: HashMap<String, Vec<String>> = HashMap::new();
    for (file, kind) in findings {
        by_file.entry(file.clone()).or_default().push(kind.clone());
    }

    let mut out = String::from("classDiagram\n");
    let mut files: Vec<String> = by_file.keys().cloned().collect();
    files.sort();

    for file in &files {
        let class_name = format!("C{}", safe_id(file).chars().take(28).collect::<String>());
        let kinds = &by_file[file];
        out.push_str(&format!("    class {} {{\n", class_name));
        out.push_str(&format!("        <<{}>> \n", file));
        for kind in kinds.iter().take(6) {
            out.push_str(&format!("        +{}\n", kind));
        }
        if kinds.len() > 6 {
            out.push_str(&format!("        +…{} more\n", kinds.len() - 6));
        }
        out.push_str("    }\n");
    }

    fence(&out, no_fence)
}
