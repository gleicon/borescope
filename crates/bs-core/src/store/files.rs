use crate::{model::LangId, Result};
use rusqlite::params;
use super::{Store, OptionalExt};

impl Store {
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

    pub fn file_loc(&self, path: &str) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT loc FROM files WHERE path=?1", params![path], |r| {
                r.get::<_, i64>(0)
            })
            .map(|v| v as u32)
            .unwrap_or(0))
    }

    pub fn file_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))? as u64)
    }

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

    /// Remove every file (and its symbols, edges, git_stats, cochange pairs) whose path
    /// is not present in `existing_paths`. Called after each index walk to evict deleted files.
    pub fn purge_deleted_files(
        &self,
        existing_paths: &std::collections::HashSet<String>,
    ) -> Result<usize> {
        let all_paths: Vec<String> = {
            let mut stmt = self.conn.prepare("SELECT path FROM files")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut purged = 0;
        for path in all_paths {
            if !existing_paths.contains(&path) {
                self.delete_file_cascade(&path)?;
                purged += 1;
            }
        }
        Ok(purged)
    }

    fn delete_file_cascade(&self, path: &str) -> Result<()> {
        let file_id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM files WHERE path=?1", params![path], |r| {
                r.get(0)
            })
            .optional()?;
        let fid = match file_id {
            Some(id) => id,
            None => return Ok(()),
        };
        // FK OFF in schema — delete in dependency order manually.
        self.conn.execute(
            "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file_id=?1)",
            params![fid],
        )?;
        self.conn.execute(
            "DELETE FROM edges WHERE to_id IN (SELECT id FROM symbols WHERE file_id=?1)",
            params![fid],
        )?;
        self.conn
            .execute("DELETE FROM symbols WHERE file_id=?1", params![fid])?;
        self.conn
            .execute("DELETE FROM git_stats WHERE file_id=?1", params![fid])?;
        self.conn.execute(
            "DELETE FROM cochange WHERE file_a_id=?1 OR file_b_id=?1",
            params![fid],
        )?;
        self.conn
            .execute("DELETE FROM files WHERE id=?1", params![fid])?;
        Ok(())
    }
}
