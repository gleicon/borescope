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
    for sym in &symbols {
        by_name
            .entry(sym.name.clone())
            .or_default()
            .push(sym.id.clone());
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

    for (from_id, unresolved_to) in &unresolved {
        let callee_name = unresolved_to.trim_start_matches("unresolved:");
        let candidates = by_name.get(callee_name);

        match candidates {
            None => {
                left_unresolved += 1;
            }
            Some(ids) if ids.len() == 1 => {
                // Unique name match — medium-high confidence
                let confidence = 0.7f32;
                store.upsert_edge(from_id, &ids[0], &EdgeKind::Calls, confidence, None)?;
                // Remove unresolved edge
                store.conn.execute(
                    "DELETE FROM edges WHERE from_id=?1 AND to_id=?2",
                    rusqlite::params![from_id, unresolved_to],
                )?;
                resolved += 1;
            }
            Some(ids) => {
                // Multiple candidates — low confidence per edge
                let confidence = 0.3f32;
                for id in ids {
                    store.upsert_edge(from_id, id, &EdgeKind::Calls, confidence, None)?;
                }
                store.conn.execute(
                    "DELETE FROM edges WHERE from_id=?1 AND to_id=?2",
                    rusqlite::params![from_id, unresolved_to],
                )?;
                left_unresolved += 1;
            }
        }
    }

    Ok(LinkStats {
        resolved,
        left_unresolved,
    })
}

pub struct LinkStats {
    pub resolved: usize,
    pub left_unresolved: usize,
}
