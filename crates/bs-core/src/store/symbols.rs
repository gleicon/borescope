use crate::{
    model::{LangId, Symbol, SymbolKind},
    Result,
};
use rusqlite::params;
use serde_json;
use super::{Store, OptionalExt};

impl Store {
    pub fn upsert_symbol(&self, sym: &Symbol) -> Result<()> {
        let file_id = self.file_id(sym.file.to_str().unwrap_or(""))?.unwrap_or(0);
        self.conn.execute(
            "INSERT INTO symbols(id,kind,name,qualified,file_id,span_start,span_end,lang,churn,age_days,loc,complexity,hotspot)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(id) DO UPDATE SET
               kind=excluded.kind, name=excluded.name, qualified=excluded.qualified,
               file_id=excluded.file_id, span_start=excluded.span_start, span_end=excluded.span_end,
               lang=excluded.lang, churn=excluded.churn, age_days=excluded.age_days,
               loc=excluded.loc, complexity=excluded.complexity, hotspot=excluded.hotspot",
            params![
                sym.id, sym.kind.to_string(), sym.name, sym.qualified,
                file_id, sym.span.0, sym.span.1, sym.lang.to_string(),
                sym.churn, sym.age_days, sym.loc, sym.complexity, sym.hotspot
            ],
        )?;
        Ok(())
    }

    pub fn update_symbol_patterns(&self, id: &str, patterns_json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET patterns=?2 WHERE id=?1",
            params![id, patterns_json],
        )?;
        Ok(())
    }

    pub fn update_symbol_signals(&self, id: &str, churn: u32, hotspot: f32) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET churn=?2, hotspot=?3 WHERE id=?1",
            params![id, churn, hotspot],
        )?;
        Ok(())
    }

    pub fn get_symbol(&self, id: &str) -> Result<Option<Symbol>> {
        let row = self
            .conn
            .query_row(
                "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot
             FROM symbols s JOIN files f ON f.id=s.file_id
             WHERE s.id=?1",
                params![id],
                symbol_from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Fetch a single symbol with its patterns column populated.
    pub fn get_symbol_with_patterns(&self, id: &str) -> Result<Option<Symbol>> {
        let row = self
            .conn
            .query_row(
                "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                        s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot,s.patterns
                 FROM symbols s JOIN files f ON f.id=s.file_id WHERE s.id=?1",
                params![id],
                |r| {
                    let mut sym = symbol_from_row(r)?;
                    let pat_str: String = r.get(13).unwrap_or_default();
                    sym.patterns = if pat_str.is_empty() {
                        vec![]
                    } else {
                        serde_json::from_str(&pat_str).unwrap_or_default()
                    };
                    Ok(sym)
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot
             FROM symbols s JOIN files f ON f.id=s.file_id
             WHERE s.name=?1 OR s.qualified=?1",
        )?;
        let rows = stmt.query_map(params![name], symbol_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn find_symbol_at_line(&self, file: &str, line: u32) -> Result<Option<Symbol>> {
        let row = self
            .conn
            .query_row(
                "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot
             FROM symbols s JOIN files f ON f.id=s.file_id
             WHERE f.path=?1 AND s.span_start<=?2 AND s.span_end>=?2
             ORDER BY (s.span_end-s.span_start) ASC
             LIMIT 1",
                params![file, line],
                symbol_from_row,
            )
            .optional()?;
        Ok(row)
    }

    pub fn symbols_for_file(&self, file: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot
             FROM symbols s JOIN files f ON f.id=s.file_id
             WHERE f.path=?1
             ORDER BY s.span_start",
        )?;
        let rows = stmt.query_map(params![file], symbol_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn all_symbols(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot
             FROM symbols s JOIN files f ON f.id=s.file_id",
        )?;
        let rows = stmt.query_map([], symbol_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn symbol_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

    /// Returns symbols that have at least one captured pattern — used by smells detectors.
    /// Pre-filtered in SQL to avoid loading the full symbol table when patterns are sparse.
    pub fn all_symbols_with_patterns(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot,s.patterns
             FROM symbols s JOIN files f ON f.id=s.file_id
             WHERE s.patterns != ''",
        )?;
        let rows = stmt.query_map([], |r| {
            let mut sym = symbol_from_row(r)?;
            let pat_str: String = r.get(13).unwrap_or_default();
            sym.patterns = if pat_str.is_empty() {
                vec![]
            } else {
                serde_json::from_str(&pat_str).unwrap_or_default()
            };
            Ok(sym)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

pub(super) fn symbol_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Symbol> {
    symbol_from_row_n(r, 0)
}

pub(super) fn symbol_from_row_n(r: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Symbol> {
    let kind_str: String = r.get(offset + 1)?;
    let lang_str: String = r.get(offset + 7)?;
    let path_str: String = r.get(offset + 4)?;
    Ok(Symbol {
        id: r.get(offset)?,
        kind: kind_str.parse().unwrap_or(SymbolKind::Function),
        name: r.get(offset + 2)?,
        qualified: r.get(offset + 3)?,
        file: std::path::PathBuf::from(path_str),
        span: (
            r.get::<_, i64>(offset + 5)? as u32,
            r.get::<_, i64>(offset + 6)? as u32,
        ),
        lang: lang_str.parse().unwrap_or(LangId::Unknown),
        churn: r.get::<_, i64>(offset + 8)? as u32,
        age_days: r.get::<_, i64>(offset + 9)? as u32,
        loc: r.get::<_, i64>(offset + 10)? as u32,
        complexity: r.get::<_, i64>(offset + 11)? as u32,
        hotspot: r.get(offset + 12)?,
        patterns: vec![],
    })
}
