use crate::{model::{CoChange, FileStat, LangId}, util::is_test_path, Result};
use rusqlite::params;
use super::Store;

impl Store {
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

    /// Return the top `top_n` hotspot files ordered by hotspot score descending.
    /// When `exclude_tests` is true, all rows are fetched (LIMIT -1) and filtered in Rust
    /// before truncating — file counts are bounded and a SQL LIMIT before filtering would
    /// silently under-deliver in test-heavy repos.
    pub fn get_hotspots(&self, top_n: usize, exclude_tests: bool) -> Result<Vec<FileStat>> {
        // LIMIT -1 = no limit in SQLite; used when we need to filter in Rust first.
        let limit = if exclude_tests { -1i64 } else { top_n as i64 };
        let mut stmt = self.conn.prepare(
            "SELECT f.path, f.lang, f.loc, g.churn, g.age_days,
                    g.last_commit_sha, g.last_commit_ts, g.hotspot
             FROM files f JOIN git_stats g ON g.file_id=f.id
             ORDER BY g.hotspot DESC, g.churn DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], file_stat_from_row)?;
        let all: Vec<FileStat> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        if exclude_tests {
            Ok(all
                .into_iter()
                .filter(|f| !is_test_path(&f.path))
                .take(top_n)
                .collect())
        } else {
            Ok(all)
        }
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

    /// Return co-change pairs involving `path`. `min_strength` is a Jaccard coefficient
    /// (0.0–1.0); `min_support` is the minimum number of commits where both files changed.
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
        let rows = stmt.query_map(params![path, min_support, min_strength], |r| {
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

    /// Return all files ordered by age (oldest first). The `zoom` parameter is accepted for
    /// API compatibility but ignored in M0 — granularity is always file-level until M1+.
    pub fn get_age_view(&self, zoom: &str) -> Result<Vec<FileStat>> {
        let _ = zoom;
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
