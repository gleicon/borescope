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
        // Minimum count of 1: inferno drops zero-count stacks entirely.
        // When no weight is chosen, every reachable path counts as 1 occurrence.
        let weight_int = ((node.weight * 1000.0).round() as u64).max(1);
        out.push_str(&format!("{} {}\n", stack.join(";"), weight_int));
    } else {
        for child in &node.children {
            emit_paths(out, child, stack);
        }
    }
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TreeNode;

    fn leaf(name: &str, weight: f32) -> TreeNode {
        let mut n = TreeNode::leaf(
            name.to_string(),
            name.to_string(),
            name.to_string(),
            String::new(),
            (0, 0),
        );
        n.weight = weight;
        n
    }

    fn inner(name: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut n = leaf(name, 0.0);
        n.children = children;
        n
    }

    #[test]
    fn test_zero_weight_emits_count_one() {
        // Weight::None gives 0.0 — must still produce count ≥ 1 for inferno
        let tree = vec![inner("root", vec![leaf("child", 0.0)])];
        let out = render_folded(&tree);
        assert_eq!(out.trim(), "root;child 1");
    }

    #[test]
    fn test_weighted_leaf_scales_to_1000() {
        let tree = vec![inner("root", vec![leaf("child", 1.0)])];
        let out = render_folded(&tree);
        assert_eq!(out.trim(), "root;child 1000");
    }

    #[test]
    fn test_each_leaf_path_on_own_line() {
        let tree = vec![inner("root", vec![leaf("a", 0.5), leaf("b", 0.0)])];
        let out = render_folded(&tree);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("root;a "));
        assert!(lines[1].starts_with("root;b "));
        // b has weight 0 but must still get count ≥ 1
        let b_count: u64 = lines[1].split_whitespace().nth(1).unwrap().parse().unwrap();
        assert!(b_count >= 1);
    }
}
