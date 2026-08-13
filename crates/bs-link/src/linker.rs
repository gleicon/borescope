//! Resolves unresolved call edges into concrete symbol-to-symbol edges
//! with confidence scores per the rubric in SPEC.md §6.
//!
//! Called after bs-extract has populated symbols and raw edges.

use bs_core::{EdgeKind, Result, Store};
use std::collections::HashMap;

pub fn link(store: &Store) -> Result<LinkStats> {
    let symbols = store.all_symbols()?;

    // name -> [symbol_ids]
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    // (name, lang_str) -> [symbol_ids] — same-language candidates preferred
    let mut by_name_lang: HashMap<(String, String), Vec<String>> = HashMap::new();
    // id -> lang_str — for caller language lookup
    let mut symbol_lang: HashMap<String, String> = HashMap::new();

    for sym in &symbols {
        by_name
            .entry(sym.name.clone())
            .or_default()
            .push(sym.id.clone());
        by_name_lang
            .entry((sym.name.clone(), sym.lang.to_string()))
            .or_default()
            .push(sym.id.clone());
        symbol_lang.insert(sym.id.clone(), sym.lang.to_string());
    }

    // Resolve unresolved: edges (from_id, "unresolved:<name>") -> real edges
    let unresolved: Vec<(String, String)> = {
        let mut stmt = store
            .conn
            .prepare("SELECT from_id, to_id FROM edges WHERE to_id LIKE 'unresolved:%'")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut resolved = 0usize;
    let mut left_unresolved = 0usize;
    let mut external = 0usize;

    for (from_id, unresolved_to) in &unresolved {
        let callee_name = unresolved_to.trim_start_matches("unresolved:");

        // Look for same-language candidates first
        let caller_lang = symbol_lang
            .get(from_id.as_str())
            .cloned()
            .unwrap_or_default();
        let same_lang_candidates = by_name_lang.get(&(callee_name.to_string(), caller_lang));

        enum Resolution {
            Unique(Vec<String>, f32),
            Ambiguous(Vec<String>, f32),
            External,
        }

        let resolution = match same_lang_candidates {
            Some(ids) if ids.len() == 1 => Resolution::Unique(ids.clone(), 0.7),
            Some(ids) => Resolution::Ambiguous(ids.clone(), 0.3),
            None => {
                // No same-lang match — try any-lang (possible cross-language call or stdlib shim)
                match by_name.get(callee_name) {
                    Some(ids) if ids.len() == 1 => Resolution::Unique(ids.clone(), 0.5),
                    Some(ids) => Resolution::Ambiguous(ids.clone(), 0.2),
                    // No candidates → external (stdlib, OS, unindexed dep)
                    None => Resolution::External,
                }
            }
        };

        match resolution {
            Resolution::Unique(ids, conf) => {
                for id in &ids {
                    store.upsert_edge(from_id, id, &EdgeKind::Calls, conf, None)?;
                }
                store.conn.execute(
                    "DELETE FROM edges WHERE from_id=?1 AND to_id=?2",
                    rusqlite::params![from_id, unresolved_to],
                )?;
                resolved += 1;
            }
            Resolution::Ambiguous(ids, conf) => {
                for id in &ids {
                    store.upsert_edge(from_id, id, &EdgeKind::Calls, conf, None)?;
                }
                store.conn.execute(
                    "DELETE FROM edges WHERE from_id=?1 AND to_id=?2",
                    rusqlite::params![from_id, unresolved_to],
                )?;
                left_unresolved += 1;
            }
            Resolution::External => {
                // Mark as external rather than leaving as unresolved
                let external_id = format!("external:{}", callee_name);
                store.upsert_edge(from_id, &external_id, &EdgeKind::Calls, 0.0, None)?;
                store.conn.execute(
                    "DELETE FROM edges WHERE from_id=?1 AND to_id=?2",
                    rusqlite::params![from_id, unresolved_to],
                )?;
                external += 1;
            }
        }
    }

    Ok(LinkStats {
        resolved,
        left_unresolved,
        external,
    })
}

pub struct LinkStats {
    pub resolved: usize,
    pub left_unresolved: usize,
    pub external: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::{model::SymbolKind, EdgeKind, LangId, Store, Symbol};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn open_store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn sym(id: &str, name: &str, lang: LangId) -> Symbol {
        Symbol {
            id: id.to_string(),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qualified: format!("src/lib.rs:{}", name),
            file: PathBuf::from("src/lib.rs"),
            span: (1, 5),
            lang,
            churn: 0,
            age_days: 0,
            loc: 5,
            complexity: 1,
            hotspot: 0.0,
            patterns: vec![],
        }
    }

    fn insert_sym(store: &Store, s: &Symbol) {
        store
            .upsert_file(s.file.to_str().unwrap(), &s.lang, s.loc)
            .unwrap();
        store.upsert_symbol(s).unwrap();
    }

    fn unresolved_edge(store: &Store, from: &str, callee_name: &str) {
        store
            .upsert_edge(from, &format!("unresolved:{}", callee_name), &EdgeKind::Calls, 0.3, None)
            .unwrap();
    }

    fn edge_conf(store: &Store, from: &str, to: &str) -> Option<f32> {
        let mut stmt = store
            .conn
            .prepare("SELECT confidence FROM edges WHERE from_id=?1 AND to_id=?2")
            .unwrap();
        stmt.query_row(rusqlite::params![from, to], |r| r.get(0)).ok()
    }

    #[test]
    fn same_lang_unique_resolves_at_0_7() {
        let (_dir, store) = open_store();
        let a = sym("a", "alpha", LangId::Rust);
        let b = sym("b", "beta", LangId::Rust);
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        unresolved_edge(&store, "a", "beta");

        let stats = link(&store).unwrap();
        assert_eq!(stats.resolved, 1);
        assert_eq!(stats.external, 0);
        let conf = edge_conf(&store, "a", "b").unwrap();
        assert!((conf - 0.7).abs() < 0.01, "expected 0.7, got {}", conf);
    }

    #[test]
    fn cross_lang_unique_resolves_at_0_5() {
        let (_dir, store) = open_store();
        let a = sym("a", "alpha", LangId::Rust);
        let b = sym("b", "beta", LangId::Go);
        insert_sym(&store, &a);
        insert_sym(&store, &b);
        unresolved_edge(&store, "a", "beta");

        link(&store).unwrap();
        let conf = edge_conf(&store, "a", "b").unwrap();
        assert!((conf - 0.5).abs() < 0.01, "expected 0.5, got {}", conf);
    }

    #[test]
    fn no_candidates_becomes_external() {
        let (_dir, store) = open_store();
        let a = sym("a", "alpha", LangId::Rust);
        insert_sym(&store, &a);
        unresolved_edge(&store, "a", "stdlib_func");

        let stats = link(&store).unwrap();
        assert_eq!(stats.external, 1);
        assert_eq!(stats.resolved, 0);
        assert!(edge_conf(&store, "a", "external:stdlib_func").is_some());
    }

    #[test]
    fn ambiguous_same_lang_resolves_at_0_3() {
        let (_dir, store) = open_store();
        let a = sym("a", "alpha", LangId::Rust);
        let b1 = sym("b1", "shared", LangId::Rust);
        let mut b2 = sym("b2", "shared", LangId::Rust);
        b2.file = PathBuf::from("src/other.rs");
        insert_sym(&store, &a);
        insert_sym(&store, &b1);
        insert_sym(&store, &b2);
        unresolved_edge(&store, "a", "shared");

        link(&store).unwrap();
        // Both candidates get edges at 0.3
        let c1 = edge_conf(&store, "a", "b1");
        let c2 = edge_conf(&store, "a", "b2");
        assert!(c1.is_some() || c2.is_some());
        for c in [c1, c2].into_iter().flatten() {
            assert!((c - 0.3).abs() < 0.01, "expected 0.3, got {}", c);
        }
    }
}
