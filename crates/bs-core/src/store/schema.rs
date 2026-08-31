use crate::Result;
use rusqlite::params;
use super::Store;

const SCHEMA_VERSION: u32 = 1;

pub(super) const SCHEMA: &str = r#"
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

impl Store {
    pub(super) fn init_schema(&self) -> Result<()> {
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
}
