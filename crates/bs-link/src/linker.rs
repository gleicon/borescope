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
    // (name, lang_str) -> [symbol_ids] — D1: same-language candidates preferred
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

        // D1: look for same-language candidates first
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
                    // D11: no candidates at all → external (stdlib, OS, unindexed dep)
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
                // D11: mark as external rather than leaving as unresolved
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
