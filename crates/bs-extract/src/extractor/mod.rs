mod complexity;
mod write;

use self::complexity::compute_complexity;
use self::write::write_parsed;

use crate::{
    language::lang_config,
    queries::{
        DEF_FUNCTION, DEF_METHOD, DEF_TYPE, IMPORT, PATTERN_PREFIX, REF_CALL, REF_CALL_RECEIVER,
        REF_ITEM,
    },
};
use bs_core::{LangId, Result, Store, SymbolKind};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub files_processed: usize,
    pub symbols_found: usize,
    pub files_skipped: usize,
}

struct ParsedFile {
    rel_path: String,
    lang: LangId,
    loc: u32,
    defs: Vec<RawDef>,
    calls: Vec<RawCall>,
    refs: Vec<RawRef>,
    imports: Vec<String>,
    patterns: Vec<RawPattern>,
    file_hash: String,
}

pub(super) struct RawDef {
    pub(super) name: String,
    pub(super) kind: SymbolKind,
    pub(super) start_line: u32,
    pub(super) end_line: u32,
    pub(super) complexity: u32,
}

pub(super) struct RawCall {
    pub(super) callee: String,
    pub(super) receiver: Option<String>,
    pub(super) line: u32,
}

pub(super) struct RawRef {
    pub(super) name: String,
    pub(super) line: u32,
}

pub(super) struct RawPattern {
    pub(super) kind: String,
    pub(super) line: u32,
}

pub fn extract_repo(
    store: &Store,
    repo_root: &Path,
    grammar_path: Option<&Path>,
) -> Result<ExtractionResult> {
    let grammar_dir = grammar_path;
    let query_override_dir = repo_root.join(".borescope").join("queries");
    let query_override = if query_override_dir.is_dir() {
        Some(query_override_dir.as_path().to_path_buf())
    } else {
        None
    };

    let stored_hashes = store.get_file_hashes().unwrap_or_default();

    let files: Vec<PathBuf> = WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();

    let current_source_paths: std::collections::HashSet<String> = files
        .iter()
        .filter(|p| LangId::from_path(p).is_source())
        .filter_map(|p| {
            p.strip_prefix(repo_root)
                .ok()
                .map(|r| r.to_string_lossy().into_owned())
        })
        .collect();
    let _ = store.purge_deleted_files(&current_source_paths);

    // Phase 1 — parse in parallel (CPU-bound, no DB access)
    let parsed: Vec<Result<Option<ParsedFile>>> = files
        .par_iter()
        .map(|file_path| {
            let lang = LangId::from_path(file_path);
            if !lang.is_source() {
                return Ok(None);
            }
            let rel = file_path
                .strip_prefix(repo_root)
                .unwrap_or(file_path)
                .to_string_lossy()
                .into_owned();

            let current_hash = file_fingerprint(file_path);
            if let Some(stored) = stored_hashes.get(&rel) {
                if *stored == current_hash {
                    return Ok(None);
                }
            }

            parse_file(file_path, &rel, &lang, grammar_dir, query_override.as_deref())
                .map(|mut pf| {
                    pf.file_hash = current_hash;
                    Some(pf)
                })
        })
        .collect();

    // Phase 2 — write to DB serially (SQLite serializes writers)
    let mut result = ExtractionResult::default();
    for outcome in parsed {
        match outcome {
            Ok(None) => result.files_skipped += 1,
            Ok(Some(pf)) => match write_parsed(store, pf) {
                Ok(n) => {
                    result.files_processed += 1;
                    result.symbols_found += n;
                }
                Err(_) => result.files_skipped += 1,
            },
            Err(_) => result.files_skipped += 1,
        }
    }

    Ok(result)
}

/// Stable file fingerprint: mtime nanoseconds + file size, no content read.
fn file_fingerprint(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("{}:{}", mtime, m.len())
        }
        Err(_) => String::new(),
    }
}

pub fn extract_file(
    store: &Store,
    abs_path: &Path,
    rel_path: &str,
    lang: &LangId,
) -> Result<usize> {
    let pf = parse_file(abs_path, rel_path, lang, None, None)?;
    write_parsed(store, pf)
}

fn parse_file(
    abs_path: &Path,
    rel_path: &str,
    lang: &LangId,
    grammar_dir: Option<&Path>,
    query_override_dir: Option<&Path>,
) -> Result<ParsedFile> {
    let cfg = if let Some(dir) = grammar_dir {
        let lang_name = lang.to_string();
        crate::language::load_dynamic_grammar(dir, &lang_name)
            .or_else(|| lang_config(lang, query_override_dir))
    } else {
        lang_config(lang, query_override_dir)
    };

    let cfg = match cfg {
        Some(c) => c,
        None => {
            return Ok(ParsedFile {
                rel_path: rel_path.to_string(),
                lang: lang.clone(),
                loc: 0,
                defs: vec![],
                calls: vec![],
                refs: vec![],
                imports: vec![],
                patterns: vec![],
                file_hash: String::new(),
            })
        }
    };

    let source = std::fs::read(abs_path)?;
    let loc = source.iter().filter(|&&b| b == b'\n').count() as u32 + 1;

    let mut parser = Parser::new();
    parser
        .set_language(&cfg.ts_language)
        .map_err(|e| bs_core::Error::Parse(e.to_string()))?;

    let tree = match parser.parse(&source, None) {
        Some(t) => t,
        None => {
            return Ok(ParsedFile {
                rel_path: rel_path.to_string(),
                lang: lang.clone(),
                loc,
                defs: vec![],
                calls: vec![],
                refs: vec![],
                imports: vec![],
                patterns: vec![],
                file_hash: String::new(),
            })
        }
    };

    let query = match Query::new(&cfg.ts_language, &cfg.query_source) {
        Ok(q) => q,
        Err(e) => return Err(bs_core::Error::Parse(e.to_string())),
    };

    let cap_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_slice());

    let mut defs: Vec<RawDef> = Vec::new();
    let mut calls: Vec<RawCall> = Vec::new();
    let mut refs: Vec<RawRef> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut patterns: Vec<RawPattern> = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let cap_name = &cap_names[cap.index as usize];
            let node = cap.node;
            let text = node.utf8_text(&source).unwrap_or("").trim().to_string();
            let start_line = node.start_position().row as u32 + 1;

            if cap_name == DEF_FUNCTION || cap_name == DEF_METHOD || cap_name == DEF_TYPE {
                let kind = match cap_name.as_str() {
                    DEF_FUNCTION => SymbolKind::Function,
                    DEF_METHOD => SymbolKind::Method,
                    _ => SymbolKind::Type,
                };
                let body = node.parent().unwrap_or(node);
                let body_start = body.start_position().row as u32 + 1;
                let body_end = body.end_position().row as u32 + 1;
                let complexity = compute_complexity(&tree, &source, body_start, body_end);
                defs.push(RawDef { name: text, kind, start_line: body_start, end_line: body_end, complexity });
            } else if cap_name == REF_CALL {
                calls.push(RawCall { callee: text, receiver: None, line: start_line });
            } else if cap_name == REF_CALL_RECEIVER {
                if let Some(last) = calls.last_mut() {
                    last.receiver = Some(text);
                }
            } else if cap_name == REF_ITEM {
                refs.push(RawRef { name: text, line: start_line });
            } else if cap_name == IMPORT {
                imports.push(text);
            } else if let Some(stripped) = cap_name.strip_prefix(PATTERN_PREFIX) {
                patterns.push(RawPattern { kind: stripped.to_string(), line: start_line });
            }
        }
    }

    Ok(ParsedFile {
        rel_path: rel_path.to_string(),
        lang: lang.clone(),
        loc,
        defs,
        calls,
        refs,
        imports,
        patterns,
        file_hash: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::{LangId, Store};

    #[test]
    fn test_rust_pattern_capture() {
        let dir = std::env::temp_dir();
        let src = dir.join("_btest_patterns.rs");
        let db = dir.join("_btest_patterns");
        let _ = std::fs::remove_file(&db);

        let code = r#"
async fn has_lock_and_await(queue: std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
    let guard = queue.lock().await;
    drop(guard);
}

fn has_loop() {
    for i in 0..10 {
        let _ = i;
    }
}
"#;
        std::fs::write(&src, code).unwrap();

        let store = Store::open(&db).unwrap();
        extract_file(&store, &src, "_btest_patterns.rs", &LangId::Rust).unwrap();

        let syms = store.all_symbols_with_patterns().unwrap();
        let lock_await = syms.iter().find(|s| s.name == "has_lock_and_await");
        let loopy = syms.iter().find(|s| s.name == "has_loop");

        assert!(lock_await.is_some(), "has_lock_and_await not found");
        let la = lock_await.unwrap();
        assert!(la.patterns.contains(&"lock".to_string()), "missing lock, got {:?}", la.patterns);
        assert!(la.patterns.contains(&"await".to_string()), "missing await, got {:?}", la.patterns);

        assert!(loopy.is_some(), "has_loop not found");
        let lp = loopy.unwrap();
        assert!(lp.patterns.contains(&"loop".to_string()), "missing loop, got {:?}", lp.patterns);
    }

    #[test]
    fn test_call_extraction_simple() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("lib.rs");
        let code = r#"pub fn alpha() -> u32 {
    beta() + gamma()
}
pub fn beta() -> u32 { gamma() * 2 }
pub fn gamma() -> u32 { 42 }
"#;
        std::fs::write(&src, code).unwrap();
        let store = Store::open(tmp.path()).unwrap();
        extract_file(&store, &src, "lib.rs", &LangId::Rust).unwrap();
        let syms = store.all_symbols().unwrap();
        let alpha = syms.iter().find(|s| s.name == "alpha").expect("alpha not found");
        let mut stmt = store
            .conn
            .prepare("SELECT to_id FROM edges WHERE from_id=?1 AND kind='calls'")
            .unwrap();
        let callee_ids: Vec<String> = stmt
            .query_map([&alpha.id], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(!callee_ids.is_empty(), "alpha should have unresolved call edges; got none");
        assert!(
            callee_ids.iter().any(|id| id == "unresolved:beta"),
            "alpha must call beta; callee_ids={callee_ids:?}"
        );
    }
}
