pub mod age;
pub mod branch;
pub mod callers;
pub mod coupled;
pub mod diff;
pub mod explain;
pub mod explain_pr;
pub mod hotspots;
pub mod index;
pub mod map;
pub mod paths;
pub mod smells;

use anyhow::{bail, Result};
use bs_core::Store;
use bs_render::{OutputFormat, Weight};
use std::path::{Path, PathBuf};

pub struct Context {
    pub repo_root: PathBuf,
    pub depth: u32,
    pub zoom: String,
    pub weight: Weight,
    pub min_confidence: f32,
    pub output: OutputFormat,
    pub no_color: bool,
    pub quiet: bool,
    pub verbose: bool,
}

pub fn resolve_repo(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return Ok(p.to_path_buf());
    }
    // Walk up from cwd looking for .git
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => bail!("not inside a git repository; use --repo to specify root"),
        }
    }
}

pub fn open_store(ctx: &Context) -> Result<Store> {
    let store = Store::open_existing(&ctx.repo_root)?;
    Ok(store)
}

/// Parse a target string "path/to/file.go:Symbol" or "path/to/file.go:42" or "QualifiedName"
pub struct Target {
    pub file: Option<String>,
    pub name_or_line: String,
}

impl Target {
    pub fn parse(s: &str) -> Self {
        if let Some(colon_pos) = s.rfind(':') {
            let file = s[..colon_pos].to_string();
            let after = s[colon_pos + 1..].to_string();
            if !file.is_empty() && (file.contains('/') || file.contains('.')) {
                return Self {
                    file: Some(file),
                    name_or_line: after,
                };
            }
        }
        Self {
            file: None,
            name_or_line: s.to_string(),
        }
    }
}

pub fn resolve_target(store: &Store, target: &str) -> Result<bs_core::Symbol> {
    let t = Target::parse(target);

    // Line-number resolution
    if let (Some(ref file), Ok(line)) = (&t.file, t.name_or_line.parse::<u32>()) {
        if let Some(sym) = store.find_symbol_at_line(file, line)? {
            return Ok(sym);
        }
        bail!("no symbol at {}:{}", file, line);
    }

    // Name resolution
    let name = &t.name_or_line;
    let mut candidates = store.find_symbols_by_name(name)?;

    // Narrow by file if provided
    if let Some(ref file) = t.file {
        let file_candidates: Vec<_> = candidates
            .iter()
            .filter(|s| s.file.to_str().unwrap_or("") == file.as_str())
            .cloned()
            .collect();
        if !file_candidates.is_empty() {
            candidates = file_candidates;
        }
    }

    match candidates.len() {
        0 => bail!("unknown target: {}", target),
        1 => Ok(candidates.remove(0)),
        _ => {
            // Deduplicate by qualified name — same file:name but different kinds (fn vs method)
            // collapse to a single result rather than forcing the user to disambiguate.
            let mut seen = std::collections::HashSet::new();
            candidates.retain(|s| seen.insert(s.qualified.clone()));
            if candidates.len() == 1 {
                return Ok(candidates.remove(0));
            }
            let n = candidates.len();
            let json = serde_json::to_string(
                &candidates
                    .iter()
                    .map(|s| format!("{}  ({})", s.qualified, s.kind))
                    .collect::<Vec<_>>(),
            )?;
            eprintln!("{}", json);
            Err(bs_core::Error::AmbiguousTarget(target.to_string(), n).into())
        }
    }
}

/// Build a callee call tree rooted at `sym`.
///
/// `path_confidence` is the cumulative product of edge confidences from the root to
/// the current node. D2: a child is pruned when `path_confidence * edge_conf < min_conf`,
/// preventing deep traversal through chains of uncertain edges.
#[allow(clippy::too_many_arguments)]
pub fn build_tree_node(
    store: &Store,
    sym: &bs_core::Symbol,
    depth: u32,
    max_depth: u32,
    min_conf: f32,
    path_confidence: f32,
    weight: Weight,
    max_weight: f32,
    visited: &mut std::collections::HashSet<String>,
) -> bs_render::TreeNode {
    let w = weight.score_symbol(sym, max_weight, max_weight);
    let mut node = bs_render::TreeNode::leaf(
        sym.id.clone(),
        sym.name.clone(),
        sym.qualified.clone(),
        sym.file.to_string_lossy().into_owned(),
        sym.span,
    );
    node.weight = w;

    if depth >= max_depth || visited.contains(&sym.id) {
        return node;
    }
    visited.insert(sym.id.clone());

    // Fetch all call edges (no per-edge floor) and apply path-product pruning (D2)
    if let Ok(callees) = store.get_callees(&sym.id, 0.0) {
        for (callee, conf) in callees {
            let child_path_conf = path_confidence * conf;
            if child_path_conf < min_conf {
                continue; // D2: cumulative confidence too low — prune subtree
            }
            let mut child = build_tree_node(
                store,
                &callee,
                depth + 1,
                max_depth,
                min_conf,
                child_path_conf,
                weight,
                max_weight,
                visited,
            );
            child.confidence = conf;
            node.children.push(child);
        }
    }

    // D11: include external (unresolvable) callees as annotated leaf nodes
    if let Ok(externals) = store.get_external_callees(&sym.id) {
        for callee_name in externals {
            let mut ext = bs_render::TreeNode::leaf(
                format!("external:{}", callee_name),
                callee_name.clone(),
                callee_name,
                String::new(),
                (0, 0),
            );
            ext.external = true;
            ext.confidence = 0.0;
            node.children.push(ext);
        }
    }

    node
}

/// Build a caller tree rooted at `sym` (inverted edges — who calls this?).
///
/// `path_confidence` is the cumulative product from root to current node (D2).
/// Callers trees do not have external nodes — external callers are outside the index.
#[allow(clippy::too_many_arguments)]
pub fn build_callers_tree_node(
    store: &Store,
    sym: &bs_core::Symbol,
    depth: u32,
    max_depth: u32,
    min_conf: f32,
    path_confidence: f32,
    weight: Weight,
    max_weight: f32,
    visited: &mut std::collections::HashSet<String>,
) -> bs_render::TreeNode {
    let w = weight.score_symbol(sym, max_weight, max_weight);
    let mut node = bs_render::TreeNode::leaf(
        sym.id.clone(),
        sym.name.clone(),
        sym.qualified.clone(),
        sym.file.to_string_lossy().into_owned(),
        sym.span,
    );
    node.weight = w;

    if depth >= max_depth || visited.contains(&sym.id) {
        return node;
    }
    visited.insert(sym.id.clone());

    if let Ok(callers) = store.get_callers(&sym.id, 0.0) {
        for (caller, conf) in callers {
            let child_path_conf = path_confidence * conf;
            if child_path_conf < min_conf {
                continue; // D2: prune low-confidence caller chains
            }
            let mut child = build_callers_tree_node(
                store,
                &caller,
                depth + 1,
                max_depth,
                min_conf,
                child_path_conf,
                weight,
                max_weight,
                visited,
            );
            child.confidence = conf;
            node.children.push(child);
        }
    }

    node
}

pub fn has_pattern(patterns: &[String], name: &str) -> bool {
    patterns.iter().any(|p| p == name)
}

pub fn emit(_ctx: &Context, content: &str) {
    print!("{}", content);
}

#[cfg(test)]
mod tests {
    use super::has_pattern;

    #[test]
    fn test_has_pattern_found() {
        let pats = vec!["lock".to_string(), "await".to_string()];
        assert!(has_pattern(&pats, "lock"));
        assert!(has_pattern(&pats, "await"));
    }

    #[test]
    fn test_has_pattern_not_found() {
        let pats = vec!["lock".to_string()];
        assert!(!has_pattern(&pats, "block_on"));
        assert!(!has_pattern(&pats, ""));
    }

    #[test]
    fn test_has_pattern_empty_slice() {
        assert!(!has_pattern(&[], "lock"));
    }

    #[test]
    fn test_has_pattern_no_prefix_match() {
        // "lock" must not match "block_on" or "locked"
        let pats = vec!["block_on".to_string(), "locked".to_string()];
        assert!(!has_pattern(&pats, "lock"));
    }
}
