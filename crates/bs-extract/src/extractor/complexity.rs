pub(super) fn compute_complexity(tree: &tree_sitter::Tree, source: &[u8], start: u32, end: u32) -> u32 {
    let branch_kinds = [
        "if_statement",
        "if_expression",
        "for_statement",
        "while_statement",
        "for_in_statement",
        "match_expression",
        "switch_statement",
        "case",
        "catch_clause",
        "binary_expression",
    ];

    let mut count = 0u32;
    let mut max_depth = 0u32;
    let mut cursor = tree.root_node().walk();

    #[allow(clippy::too_many_arguments)]
    fn visit(
        cursor: &mut tree_sitter::TreeCursor<'_>,
        _source: &[u8],
        start: u32,
        end: u32,
        branch_kinds: &[&str],
        count: &mut u32,
        max_depth: &mut u32,
        depth: u32,
    ) {
        let node = cursor.node();
        let node_line = node.start_position().row as u32 + 1;
        if node_line >= start && node_line <= end && branch_kinds.contains(&node.kind()) {
            *count += 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
        }
        if cursor.goto_first_child() {
            visit(cursor, _source, start, end, branch_kinds, count, max_depth, depth + 1);
            while cursor.goto_next_sibling() {
                visit(cursor, _source, start, end, branch_kinds, count, max_depth, depth + 1);
            }
            cursor.goto_parent();
        }
    }

    visit(&mut cursor, source, start, end, &branch_kinds, &mut count, &mut max_depth, 0);
    count + max_depth
}
