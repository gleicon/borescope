use bs_core::{Store, Symbol};
use bs_render::{Weight, TreeNode};
use std::collections::{HashSet, VecDeque};

/// BFS through call edges to find the shortest path from `start` to `end`.
/// Returns None if no path exists within `max_depth` hops at `min_conf` path product.
pub(super) fn find_path_to(
    store: &Store,
    start: &Symbol,
    end: &Symbol,
    max_depth: u32,
    min_conf: f32,
) -> Option<Vec<Symbol>> {
    if start.id == end.id {
        return Some(vec![start.clone()]);
    }

    let mut queue: VecDeque<(Symbol, Vec<Symbol>, f32)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    queue.push_back((start.clone(), vec![start.clone()], 1.0));
    visited.insert(start.id.clone());

    while let Some((current, path, path_conf)) = queue.pop_front() {
        if path.len() as u32 > max_depth {
            continue;
        }
        if let Ok(callees) = store.get_callees(&current.id, 0.0) {
            for (callee, conf) in callees {
                let child_conf = path_conf * conf;
                if child_conf < min_conf || visited.contains(&callee.id) {
                    continue;
                }
                let mut new_path = path.clone();
                new_path.push(callee.clone());
                if callee.id == end.id {
                    return Some(new_path);
                }
                visited.insert(callee.id.clone());
                queue.push_back((callee, new_path, child_conf));
            }
        }
    }

    None
}

/// Load patterns for a slice of symbols (patterns field is empty from get_callees).
pub(super) fn enrich_with_patterns(store: &Store, syms: &[Symbol]) -> Vec<Symbol> {
    syms.iter()
        .map(|s| {
            store
                .get_symbol_with_patterns(&s.id)
                .ok()
                .flatten()
                .unwrap_or_else(|| s.clone())
        })
        .collect()
}

/// Build a linear TreeNode chain from a path (root → child → grandchild → ...).
pub(super) fn build_path_tree(
    path: &[Symbol],
    max_churn: f32,
    max_loc: f32,
    weight: Weight,
) -> TreeNode {
    let make_node = |sym: &Symbol| -> TreeNode {
        let mut n = TreeNode::leaf(
            sym.id.clone(),
            sym.name.clone(),
            sym.qualified.clone(),
            sym.file.to_string_lossy().into_owned(),
            sym.span,
        );
        n.weight = weight.score_symbol(sym, max_churn, max_loc);
        n
    };

    let mut nodes: Vec<TreeNode> = path.iter().map(make_node).collect();

    // Build from tail to head: last node has no children
    let mut child: Option<TreeNode> = None;
    for node in nodes.iter_mut().rev() {
        if let Some(c) = child.take() {
            node.children.push(c);
        }
        child = Some(node.clone());
    }

    child.unwrap_or_else(|| make_node(&path[0]))
}

/// Collect all symbol IDs in a tree (DFS).
pub(super) fn collect_tree_ids(node: &TreeNode) -> Vec<String> {
    let mut ids = vec![node.id.clone()];
    for child in &node.children {
        ids.extend(collect_tree_ids(child));
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::{model::SymbolKind, EdgeKind, LangId, Store};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn tmp_store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn make_sym(id: &str, name: &str, file: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qualified: format!("{}:{}", file, name),
            file: PathBuf::from(file),
            span: (1, 10),
            lang: LangId::Rust,
            churn: 0,
            age_days: 0,
            loc: 10,
            complexity: 1,
            hotspot: 0.0,
            patterns: vec![],
        }
    }

    fn insert_sym(store: &Store, sym: &Symbol) {
        store
            .upsert_file(sym.file.to_str().unwrap(), &sym.lang, sym.loc)
            .unwrap();
        store.upsert_symbol(sym).unwrap();
    }

    fn insert_edge(store: &Store, from: &str, to: &str, conf: f32) {
        store
            .upsert_edge(from, to, &EdgeKind::Calls, conf, None)
            .unwrap();
    }

    #[test]
    fn path_to_self_returns_single_node() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        insert_sym(&store, &a);
        let result = find_path_to(&store, &a, &a, 4, 0.0);
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn path_to_direct_callee() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_edge(&store, "a", "b", 1.0);

        let path = find_path_to(&store, &a, &b, 3, 0.0).unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].id, "a");
        assert_eq!(path[1].id, "b");
    }

    #[test]
    fn path_to_two_hops() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        let c = make_sym("c", "gamma", "src/c.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_sym(&store, &c);
        insert_edge(&store, "a", "b", 1.0);
        insert_edge(&store, "b", "c", 1.0);

        let path = find_path_to(&store, &a, &c, 4, 0.0).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[2].id, "c");
    }

    #[test]
    fn path_to_none_when_unreachable() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        assert!(find_path_to(&store, &a, &b, 4, 0.0).is_none());
    }

    #[test]
    fn path_to_none_when_depth_exceeded() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        let c = make_sym("c", "gamma", "src/c.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_sym(&store, &c);
        insert_edge(&store, "a", "b", 1.0);
        insert_edge(&store, "b", "c", 1.0);
        // max_depth=1 — path has 2 hops, unreachable
        assert!(find_path_to(&store, &a, &c, 1, 0.0).is_none());
    }

    #[test]
    fn path_to_pruned_by_confidence() {
        let (_dir, store) = tmp_store();
        let a = make_sym("a", "alpha", "src/a.rs");
        let b = make_sym("b", "beta", "src/b.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        insert_edge(&store, "a", "b", 0.2); // low confidence edge
                                            // min_conf=0.5 prunes this edge
        assert!(find_path_to(&store, &a, &b, 4, 0.5).is_none());
    }
}
