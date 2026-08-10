use crate::{
    error::Error,
    model::{CoChange, EdgeKind, FileStat, LangId, Symbol, SymbolKind},
    Result,
};
use rusqlite::{params, Connection};
use serde_json;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    lang TEXT NOT NULL DEFAULT 'unknown',
    loc  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS git_stats (
    file_id        INTEGER PRIMARY KEY REFERENCES files(id),
    churn          INTEGER NOT NULL DEFAULT 0,
    age_days       INTEGER NOT NULL DEFAULT 0,
    last_commit_sha TEXT,
    last_commit_ts  INTEGER,
    hotspot        REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS cochange (
    file_a_id  INTEGER NOT NULL REFERENCES files(id),
    file_b_id  INTEGER NOT NULL REFERENCES files(id),
    support    INTEGER NOT NULL DEFAULT 0,
    strength   REAL NOT NULL DEFAULT 0.0,
    strength_rev REAL NOT NULL DEFAULT 0.0,
    PRIMARY KEY (file_a_id, file_b_id),
    CHECK (file_a_id < file_b_id)
);

CREATE TABLE IF NOT EXISTS symbols (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL,
    qualified  TEXT NOT NULL,
    file_id    INTEGER NOT NULL REFERENCES files(id),
    span_start INTEGER NOT NULL,
    span_end   INTEGER NOT NULL,
    lang       TEXT NOT NULL,
    churn      INTEGER NOT NULL DEFAULT 0,
    age_days   INTEGER NOT NULL DEFAULT 0,
    loc        INTEGER NOT NULL DEFAULT 0,
    complexity INTEGER NOT NULL DEFAULT 0,
    hotspot    REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS edges (
    from_id    TEXT NOT NULL REFERENCES symbols(id),
    to_id      TEXT NOT NULL REFERENCES symbols(id),
    kind       TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    meta       TEXT,
    PRIMARY KEY (from_id, to_id, kind)
);

CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE INDEX IF NOT EXISTS idx_git_stats_churn ON git_stats(churn DESC);
"#;

pub struct Store {
    pub conn: Connection,
    pub root: PathBuf,
}

impl Store {
    pub fn open(repo_root: &Path) -> Result<Self> {
        let dir = repo_root.join(".borescope");
        std::fs::create_dir_all(&dir)?;
        let db_path = dir.join("index.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let store = Self {
            conn,
            root: repo_root.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_existing(repo_root: &Path) -> Result<Self> {
        let db_path = repo_root.join(".borescope").join("index.db");
        if !db_path.exists() {
            return Err(Error::NoIndex);
        }
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let store = Self {
            conn,
            root: repo_root.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        let stored: Option<u32> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok());

        if stored.is_none() {
            self.conn.execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES('schema_version',?1)",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }

        // Additive migration: add patterns column if missing (schema v1 → v2)
        let has_patterns = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('symbols') WHERE name='patterns'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !has_patterns {
            self.conn.execute_batch(
                "ALTER TABLE symbols ADD COLUMN patterns TEXT NOT NULL DEFAULT ''",
            )?;
        }

        // Additive migration: add file_hash column if missing
        let has_file_hash = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('files') WHERE name='file_hash'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !has_file_hash {
            self.conn
                .execute_batch("ALTER TABLE files ADD COLUMN file_hash TEXT NOT NULL DEFAULT ''")?;
        }

        Ok(())
    }

    // ---------- meta ----------

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let v = self
            .conn
            .query_row("SELECT value FROM meta WHERE key=?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(v)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key,value) VALUES(?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------- files ----------

    pub fn get_file_hashes(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, file_hash FROM files WHERE file_hash != ''")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (path, hash) = row?;
            map.insert(path, hash);
        }
        Ok(map)
    }

    pub fn update_file_hash(&self, file_id: i64, hash: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET file_hash=?2 WHERE id=?1",
            params![file_id, hash],
        )?;
        Ok(())
    }

    pub fn upsert_file(&self, path: &str, lang: &LangId, loc: u32) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files(path,lang,loc) VALUES(?1,?2,?3)
             ON CONFLICT(path) DO UPDATE SET lang=excluded.lang, loc=excluded.loc",
            params![path, lang.to_string(), loc],
        )?;
        let id = self
            .conn
            .query_row("SELECT id FROM files WHERE path=?1", params![path], |r| {
                r.get(0)
            })?;
        Ok(id)
    }

    pub fn file_id(&self, path: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM files WHERE path=?1", params![path], |r| {
                r.get(0)
            })
            .optional()?)
    }

    // ---------- git_stats ----------

    pub fn upsert_git_stat(
        &self,
        file_id: i64,
        churn: u32,
        age_days: u32,
        sha: Option<&str>,
        ts: Option<i64>,
        hotspot: f32,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO git_stats(file_id,churn,age_days,last_commit_sha,last_commit_ts,hotspot)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(file_id) DO UPDATE SET
               churn=excluded.churn,
               age_days=excluded.age_days,
               last_commit_sha=excluded.last_commit_sha,
               last_commit_ts=excluded.last_commit_ts,
               hotspot=excluded.hotspot",
            params![file_id, churn, age_days, sha, ts, hotspot],
        )?;
        Ok(())
    }

    pub fn upsert_cochange(
        &self,
        a_id: i64,
        b_id: i64,
        support: u32,
        strength: f32,
        strength_rev: f32,
    ) -> Result<()> {
        let (lo, hi, s, sr) = if a_id < b_id {
            (a_id, b_id, strength, strength_rev)
        } else {
            (b_id, a_id, strength_rev, strength)
        };
        self.conn.execute(
            "INSERT INTO cochange(file_a_id,file_b_id,support,strength,strength_rev)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(file_a_id,file_b_id) DO UPDATE SET
               support=excluded.support,
               strength=excluded.strength,
               strength_rev=excluded.strength_rev",
            params![lo, hi, support, s, sr],
        )?;
        Ok(())
    }

    // ---------- symbols ----------

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

    // ---------- queries ----------

    pub fn get_hotspots(&self, top_n: usize) -> Result<Vec<FileStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.lang, f.loc, g.churn, g.age_days,
                    g.last_commit_sha, g.last_commit_ts, g.hotspot
             FROM files f JOIN git_stats g ON g.file_id=f.id
             ORDER BY g.hotspot DESC, g.churn DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![top_n as i64], file_stat_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_all_file_stats(&self) -> Result<Vec<FileStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.lang, f.loc, g.churn, g.age_days,
                    g.last_commit_sha, g.last_commit_ts, g.hotspot
             FROM files f JOIN git_stats g ON g.file_id=f.id",
        )?;
        let rows = stmt.query_map([], file_stat_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_coupled(
        &self,
        path: &str,
        min_strength: f32,
        min_support: u32,
    ) -> Result<Vec<CoChange>> {
        let mut stmt = self.conn.prepare(
            "SELECT fa.path, fb.path, c.support, c.strength, c.strength_rev
             FROM cochange c
             JOIN files fa ON fa.id=c.file_a_id
             JOIN files fb ON fb.id=c.file_b_id
             WHERE (fa.path=?1 OR fb.path=?1)
               AND c.support >= ?2
               AND (c.strength >= ?3 OR c.strength_rev >= ?3)
             ORDER BY c.strength DESC",
        )?;
        let target = path;
        let rows = stmt.query_map(params![target, min_support, min_strength], |r| {
            Ok(CoChange {
                file_a: r.get(0)?,
                file_b: r.get(1)?,
                support: r.get::<_, i64>(2)? as u32,
                strength: r.get(3)?,
                strength_rev: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_age_view(&self, zoom: &str) -> Result<Vec<FileStat>> {
        let _ = zoom; // file-level only in M0; zoom expansion in M1+
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.lang, f.loc, g.churn, g.age_days,
                    g.last_commit_sha, g.last_commit_ts, g.hotspot
             FROM files f JOIN git_stats g ON g.file_id=f.id
             ORDER BY g.age_days DESC",
        )?;
        let rows = stmt.query_map([], file_stat_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_map_by_churn(&self) -> Result<Vec<FileStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.lang, f.loc, g.churn, g.age_days,
                    g.last_commit_sha, g.last_commit_ts, g.hotspot
             FROM files f JOIN git_stats g ON g.file_id=f.id
             ORDER BY g.churn DESC",
        )?;
        let rows = stmt.query_map([], file_stat_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_all_cochange(&self, min_support: u32) -> Result<Vec<CoChange>> {
        let mut stmt = self.conn.prepare(
            "SELECT fa.path, fb.path, c.support, c.strength, c.strength_rev
             FROM cochange c
             JOIN files fa ON fa.id=c.file_a_id
             JOIN files fb ON fb.id=c.file_b_id
             WHERE c.support >= ?1",
        )?;
        let rows = stmt.query_map(params![min_support], |r| {
            Ok(CoChange {
                file_a: r.get(0)?,
                file_b: r.get(1)?,
                support: r.get::<_, i64>(2)? as u32,
                strength: r.get(3)?,
                strength_rev: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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

    pub fn get_callees(&self, symbol_id: &str, min_conf: f32) -> Result<Vec<(Symbol, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot, e.confidence
             FROM edges e
             JOIN symbols s ON s.id=e.to_id
             JOIN files f ON f.id=s.file_id
             WHERE e.from_id=?1 AND e.kind='calls' AND e.confidence>=?2",
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
             WHERE e.to_id=?1 AND e.kind='calls' AND e.confidence>=?2",
        )?;
        let rows = stmt.query_map(params![symbol_id, min_conf], |r| {
            let sym = symbol_from_row_n(r, 0)?;
            let conf: f32 = r.get(13)?;
            Ok((sym, conf))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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

    pub fn file_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))? as u64)
    }

    pub fn update_symbol_patterns(&self, id: &str, patterns_json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET patterns=?2 WHERE id=?1",
            params![id, patterns_json],
        )?;
        Ok(())
    }

    /// Returns all symbols with their patterns column populated — used by smells detectors.
    pub fn all_symbols_with_patterns(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id,s.kind,s.name,s.qualified,f.path,s.span_start,s.span_end,
                    s.lang,s.churn,s.age_days,s.loc,s.complexity,s.hotspot,s.patterns
             FROM symbols s JOIN files f ON f.id=s.file_id",
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

    /// Returns (fanin, fanout) per symbol id for all `calls` edges.
    pub fn get_call_edge_counts(&self) -> Result<std::collections::HashMap<String, (u32, u32)>> {
        let mut counts: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id FROM edges WHERE kind='calls'")?;
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

    pub fn update_symbol_signals(&self, id: &str, churn: u32, hotspot: f32) -> Result<()> {
        self.conn.execute(
            "UPDATE symbols SET churn=?2, hotspot=?3 WHERE id=?1",
            params![id, churn, hotspot],
        )?;
        Ok(())
    }

    pub fn file_loc(&self, path: &str) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT loc FROM files WHERE path=?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .map(|v| v as u32)
            .unwrap_or(0))
    }
}

fn file_stat_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileStat> {
    let lang_str: String = r.get(1)?;
    Ok(FileStat {
        path: r.get(0)?,
        lang: lang_str.parse().unwrap_or(LangId::Unknown),
        loc: r.get::<_, i64>(2)? as u32,
        churn: r.get::<_, i64>(3)? as u32,
        age_days: r.get::<_, i64>(4)? as u32,
        last_commit_sha: r.get(5)?,
        last_commit_ts: r.get(6)?,
        hotspot: r.get(7)?,
    })
}

fn symbol_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Symbol> {
    symbol_from_row_n(r, 0)
}

fn symbol_from_row_n(r: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Symbol> {
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

trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_temp() -> (Store, TempDir) {
        let tmp = TempDir::new().unwrap();
        let store = Store::open(tmp.path()).unwrap();
        (store, tmp)
    }

    fn make_sym(id: &str, name: &str, file: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            kind: SymbolKind::Function,
            name: name.to_string(),
            qualified: format!("{}:{}", file, name),
            file: std::path::PathBuf::from(file),
            span: (1, 10),
            lang: LangId::Rust,
            churn: 0,
            age_days: 0,
            loc: 10,
            complexity: 3,
            hotspot: 0.0,
            patterns: vec![],
        }
    }

    #[test]
    fn test_upsert_and_find_symbol() {
        let (store, _tmp) = open_temp();
        store.upsert_file("src/lib.rs", &LangId::Rust, 100).unwrap();
        let sym = make_sym("id1", "do_work", "src/lib.rs");
        store.upsert_symbol(&sym).unwrap();

        let found = store.find_symbols_by_name("do_work").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "do_work");
    }

    #[test]
    fn test_upsert_is_idempotent() {
        let (store, _tmp) = open_temp();
        store.upsert_file("src/lib.rs", &LangId::Rust, 100).unwrap();
        let sym = make_sym("id1", "fn_a", "src/lib.rs");
        store.upsert_symbol(&sym).unwrap();
        store.upsert_symbol(&sym).unwrap();
        assert_eq!(store.find_symbols_by_name("fn_a").unwrap().len(), 1);
    }

    #[test]
    fn test_patterns_round_trip() {
        let (store, _tmp) = open_temp();
        store.upsert_file("src/lib.rs", &LangId::Rust, 50).unwrap();
        let sym = make_sym("pid1", "async_fn", "src/lib.rs");
        store.upsert_symbol(&sym).unwrap();
        store
            .update_symbol_patterns("pid1", r#"["lock","await"]"#)
            .unwrap();

        let all = store.all_symbols_with_patterns().unwrap();
        let found = all.iter().find(|s| s.id == "pid1").unwrap();
        assert!(found.patterns.contains(&"lock".to_string()));
        assert!(found.patterns.contains(&"await".to_string()));
    }

    #[test]
    fn test_call_edge_counts_fanin_fanout() {
        let (store, _tmp) = open_temp();
        store.upsert_file("a.rs", &LangId::Rust, 10).unwrap();
        let caller = make_sym("caller", "caller", "a.rs");
        let callee = make_sym("callee", "callee", "a.rs");
        store.upsert_symbol(&caller).unwrap();
        store.upsert_symbol(&callee).unwrap();
        store
            .upsert_edge("caller", "callee", &EdgeKind::Calls, 1.0, None)
            .unwrap();

        let counts = store.get_call_edge_counts().unwrap();
        let (callee_fanin, _) = counts.get("callee").copied().unwrap_or((0, 0));
        let (_, caller_fanout) = counts.get("caller").copied().unwrap_or((0, 0));
        assert_eq!(callee_fanin, 1, "callee fanin must be 1");
        assert_eq!(caller_fanout, 1, "caller fanout must be 1");
    }

    #[test]
    fn test_cochange_upsert_and_query() {
        let (store, _tmp) = open_temp();
        let a = store.upsert_file("a.rs", &LangId::Rust, 10).unwrap();
        let b = store.upsert_file("b.rs", &LangId::Rust, 10).unwrap();
        store.upsert_cochange(a, b, 5, 0.8, 0.6).unwrap();

        let pairs = store.get_all_cochange(3).unwrap();
        assert_eq!(pairs.len(), 1);
        assert!((pairs[0].strength - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_schema_migration_patterns_column() {
        // Open twice — second open must not fail even though patterns column exists
        let tmp = TempDir::new().unwrap();
        let _ = Store::open(tmp.path()).unwrap();
        let _ = Store::open(tmp.path()).unwrap();
    }
}
