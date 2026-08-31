use crate::{error::Error, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

mod edges;
mod files;
mod git;
mod schema;
mod symbols;

pub struct Store {
    pub conn: Connection,
    pub root: PathBuf,
}

impl Store {
    /// Create (or reopen) the index at `repo_root/.borescope/index.db`, creating the directory
    /// and schema if they do not yet exist. Use `open_existing` when the index must already exist.
    pub fn open(repo_root: &Path) -> Result<Self> {
        let dir = repo_root.join(".borescope");
        std::fs::create_dir_all(&dir)?;
        let db_path = dir.join("index.db");
        let conn = Connection::open(&db_path)?;
        // FK OFF: edges deliberately use virtual IDs (unresolved:*, external:*, file:*)
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=OFF;",
        )?;
        let store = Self {
            conn,
            root: repo_root.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an existing index at `repo_root/.borescope/index.db`. Returns `Error::NoIndex`
    /// if the file does not exist — prefer this for read-only commands so they fail fast.
    pub fn open_existing(repo_root: &Path) -> Result<Self> {
        let db_path = repo_root.join(".borescope").join("index.db");
        if !db_path.exists() {
            return Err(Error::NoIndex);
        }
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=OFF;",
        )?;
        let store = Self {
            conn,
            root: repo_root.to_path_buf(),
        };
        store.init_schema()?;
        Ok(store)
    }

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
}

pub(super) trait OptionalExt<T> {
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
    use crate::model::{EdgeKind, LangId, Symbol, SymbolKind};
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

    #[test]
    fn test_get_hotspots_excludes_test_files() {
        let (store, _tmp) = open_temp();
        let prod_id = store
            .upsert_file("src/auth.rs", &LangId::Rust, 100)
            .unwrap();
        // test file scores higher but must be excluded when exclude_tests=true
        let test_id = store
            .upsert_file("tests/auth_test.rs", &LangId::Rust, 50)
            .unwrap();
        store
            .upsert_git_stat(prod_id, 10, 5, None, None, 0.9)
            .unwrap();
        store
            .upsert_git_stat(test_id, 20, 1, None, None, 0.99)
            .unwrap();

        let results = store.get_hotspots(5, true).unwrap();
        assert_eq!(results.len(), 1, "test file must be filtered out");
        assert_eq!(results[0].path, "src/auth.rs");
    }

    #[test]
    fn test_get_hotspots_includes_test_files_when_not_excluded() {
        let (store, _tmp) = open_temp();
        let prod_id = store
            .upsert_file("src/auth.rs", &LangId::Rust, 100)
            .unwrap();
        let test_id = store
            .upsert_file("tests/auth_test.rs", &LangId::Rust, 50)
            .unwrap();
        store
            .upsert_git_stat(prod_id, 10, 5, None, None, 0.9)
            .unwrap();
        store
            .upsert_git_stat(test_id, 20, 1, None, None, 0.99)
            .unwrap();

        let results = store.get_hotspots(5, false).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_purge_deleted_files_removes_stale_entry() {
        let (store, _tmp) = open_temp();
        store.upsert_file("src/a.rs", &LangId::Rust, 10).unwrap();
        store.upsert_file("src/b.rs", &LangId::Rust, 10).unwrap();

        // Only src/a.rs still exists on disk
        let existing: std::collections::HashSet<String> =
            ["src/a.rs".to_string()].into_iter().collect();
        let purged = store.purge_deleted_files(&existing).unwrap();

        assert_eq!(purged, 1);
        assert!(store.file_id("src/b.rs").unwrap().is_none());
        assert!(store.file_id("src/a.rs").unwrap().is_some());
    }

    #[test]
    fn test_purge_deleted_files_cascades_symbols_and_edges() {
        let (store, _tmp) = open_temp();
        store.upsert_file("src/a.rs", &LangId::Rust, 10).unwrap();
        let sym = make_sym("sym1", "work", "src/a.rs");
        store.upsert_symbol(&sym).unwrap();

        let existing: std::collections::HashSet<String> = std::collections::HashSet::new();
        store.purge_deleted_files(&existing).unwrap();

        assert!(store.find_symbols_by_name("work").unwrap().is_empty());
    }
}
