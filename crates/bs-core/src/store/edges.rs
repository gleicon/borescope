use crate::{model::{EdgeKind, Symbol}, Result};
use rusqlite::params;
use super::{Store, symbols::symbol_from_row_n};

impl Store {
    pub fn upsert_edge(
        &self,
        from: &str,
        to: &str,
        kind: &EdgeKind,
        confidence: f32,
        meta_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO edges(from_id,to_id,kind,confidence,meta)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(from_id,to_id,kind) DO UPDATE SET
               confidence=MAX(excluded.confidence,confidence),
               meta=excluded.meta",
            params![from, to, kind.to_string(), confidence, meta_json],
        )?;
        Ok(())
    }

    pub fn get_callees(&self, symbol_id: &str, min_conf: f32) -> Result<Vec<(Symbol, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot, e.confidence
             FROM edges e
             JOIN symbols s ON s.id=e.to_id
             JOIN files f ON f.id=s.file_id
             WHERE e.from_id=?1 AND e.kind IN ('calls','reference') AND e.confidence>=?2",
        )?;
        let rows = stmt.query_map(params![symbol_id, min_conf], |r| {
            let sym = symbol_from_row_n(r, 0)?;
            let conf: f32 = r.get(13)?;
            Ok((sym, conf))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_callers(&self, symbol_id: &str, min_conf: f32) -> Result<Vec<(Symbol, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot, e.confidence
             FROM edges e
             JOIN symbols s ON s.id=e.from_id
             JOIN files f ON f.id=s.file_id
             WHERE e.to_id=?1 AND e.kind IN ('calls','reference') AND e.confidence>=?2",
        )?;
        let rows = stmt.query_map(params![symbol_id, min_conf], |r| {
            let sym = symbol_from_row_n(r, 0)?;
            let conf: f32 = r.get(13)?;
            Ok((sym, conf))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Returns (fanin, fanout) per symbol id for all `calls` and `reference` edges.
    pub fn get_call_edge_counts(&self) -> Result<std::collections::HashMap<String, (u32, u32)>> {
        let mut counts: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id FROM edges WHERE kind IN ('calls','reference')")?;
        let rows = stmt.query_map([], |r| {
            let from: String = r.get(0)?;
            let to: String = r.get(1)?;
            Ok((from, to))
        })?;
        for row in rows {
            let (from, to) = row?;
            counts.entry(from).or_default().1 += 1; // fanout
            counts.entry(to).or_default().0 += 1; // fanin
        }
        Ok(counts)
    }

    /// Returns callee names for `external:` edges from this symbol — unresolvable calls.
    pub fn get_external_callees(&self, from_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT to_id FROM edges WHERE from_id=?1 AND to_id LIKE 'external:%'")?;
        let rows = stmt.query_map(params![from_id], |r| {
            let s: String = r.get(0)?;
            Ok(s.trim_start_matches("external:").to_string())
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Total count of external (unresolvable) call edges in the graph.
    pub fn count_external_edges(&self) -> Result<usize> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE to_id LIKE 'external:%'",
            [],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }
}
