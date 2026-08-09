use crate::TreeNode;

/// Renders to Brendan Gregg folded format.
/// Each root-to-leaf path is one line, frames joined by `;`, weight scaled ×1000.
pub fn render_folded(nodes: &[TreeNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        emit_paths(&mut out, node, &mut Vec::new());
    }
    out
}

fn emit_paths(out: &mut String, node: &TreeNode, stack: &mut Vec<String>) {
    stack.push(node.name.clone());
    if node.children.is_empty() {
        let weight_int = (node.weight * 1000.0).round() as u64;
        out.push_str(&format!("{} {}\n", stack.join(";"), weight_int));
    } else {
        for child in &node.children {
            emit_paths(out, child, stack);
        }
    }
    stack.pop();
}
