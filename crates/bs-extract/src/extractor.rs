use crate::{
    language::lang_config,
    queries::{
        DEF_FUNCTION, DEF_METHOD, DEF_TYPE, IMPORT, PATTERN_PREFIX, REF_CALL, REF_CALL_RECEIVER,
        REF_ITEM,
    },
};
use bs_core::{EdgeKind, LangId, Result, Store, Symbol, SymbolKind};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub files_processed: usize,
    pub symbols_found: usize,
    pub files_skipped: usize,
}

/// Parsed output from a single file, ready to write to the DB without re-parsing.
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

    // Load existing file hashes for incremental skip
    let stored_hashes = store.get_file_hashes().unwrap_or_default();

    let files: Vec<PathBuf> = WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();

    // Purge files that existed in the index but have since been deleted from disk.
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

            // Incremental skip: compare mtime+size against stored hash
            let current_hash = file_fingerprint(file_path);
            if let Some(stored) = stored_hashes.get(&rel) {
                if *stored == current_hash {
                    return Ok(None); // unchanged — skip re-parse
                }
            }

            parse_file(
                file_path,
                &rel,
                &lang,
                grammar_dir,
                query_override.as_deref(),
            )
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

/// Parse a file into raw symbols/calls/imports — no DB access, safe to run in parallel.
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

    let cap_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

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
                // Capture is on the name identifier; we need the parent (function_item /
                // struct_item / etc.) to get the full body span for complexity + pattern attribution.
                let body = node.parent().unwrap_or(node);
                let body_start = body.start_position().row as u32 + 1;
                let body_end = body.end_position().row as u32 + 1;
                let complexity = compute_complexity(&tree, &source, body_start, body_end);
                defs.push(RawDef {
                    name: text,
                    kind,
                    start_line: body_start,
                    end_line: body_end,
                    complexity,
                });
            } else if cap_name == REF_CALL {
                calls.push(RawCall {
                    callee: text,
                    receiver: None,
                    line: start_line,
                });
            } else if cap_name == REF_CALL_RECEIVER {
                if let Some(last) = calls.last_mut() {
                    last.receiver = Some(text);
                }
            } else if cap_name == REF_ITEM {
                refs.push(RawRef {
                    name: text,
                    line: start_line,
                });
            } else if cap_name == IMPORT {
                imports.push(text);
            } else if let Some(stripped) = cap_name.strip_prefix(PATTERN_PREFIX) {
                let kind = stripped.to_string();
                patterns.push(RawPattern {
                    kind,
                    line: start_line,
                });
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
        file_hash: String::new(), // set by caller after fingerprinting
    })
}

/// Write a parsed file into the DB — serial, no parsing.
fn write_parsed(store: &Store, pf: ParsedFile) -> Result<usize> {
    let file_id = store.upsert_file(&pf.rel_path, &pf.lang, pf.loc)?;
    if !pf.file_hash.is_empty() {
        store.update_file_hash(file_id, &pf.file_hash).ok();
    }
    if pf.defs.is_empty() && pf.imports.is_empty() && pf.calls.is_empty() {
        return Ok(0);
    }
    let count = write_defs(store, &pf)?;
    write_imports(store, &pf);
    write_calls(store, &pf);
    write_refs(store, &pf);
    Ok(count)
}

fn write_defs(store: &Store, pf: &ParsedFile) -> Result<usize> {
    let file_sym_id = format!("file:{}", pf.rel_path);
    let mut count = 0;
    for def in &pf.defs {
        let id = stable_id(&pf.rel_path, &def.name, &def.kind);
        let sym = Symbol {
            id: id.clone(),
            kind: def.kind.clone(),
            name: def.name.clone(),
            qualified: format!("{}:{}", pf.rel_path, def.name),
            file: PathBuf::from(&pf.rel_path),
            span: (def.start_line, def.end_line),
            lang: pf.lang.clone(),
            churn: 0,
            age_days: 0,
            loc: def.end_line.saturating_sub(def.start_line) + 1,
            complexity: def.complexity,
            hotspot: 0.0,
            patterns: vec![],
        };
        store.upsert_symbol(&sym)?;
        count += 1;
        write_def_patterns(store, &id, &pf.patterns, def.start_line, def.end_line);
        store.upsert_edge(&file_sym_id, &id, &EdgeKind::Contains, 1.0, None).ok();
    }
    Ok(count)
}

fn write_def_patterns(store: &Store, id: &str, patterns: &[RawPattern], start: u32, end: u32) {
    let mut def_patterns: Vec<String> = patterns
        .iter()
        .filter(|p| p.line >= start && p.line <= end)
        .map(|p| p.kind.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    def_patterns.sort();
    if !def_patterns.is_empty() {
        if let Ok(json) = serde_json::to_string(&def_patterns) {
            store.update_symbol_patterns(id, &json).ok();
        }
    }
}

fn write_imports(store: &Store, pf: &ParsedFile) {
    let from_id = stable_id(&pf.rel_path, &pf.rel_path, &SymbolKind::File);
    for import in &pf.imports {
        store.upsert_edge(&from_id, &format!("import:{}", import), &EdgeKind::Imports, 1.0, None).ok();
    }
}

fn write_calls(store: &Store, pf: &ParsedFile) {
    for call in &pf.calls {
        if let Some(enc) = enclosing_def(&pf.defs, call.line) {
            let enc_id = stable_id(&pf.rel_path, &enc.name, &enc.kind);
            store.upsert_edge(&enc_id, &format!("unresolved:{}", call.callee), &EdgeKind::Calls, 0.3, None).ok();
        }
    }
}

fn write_refs(store: &Store, pf: &ParsedFile) {
    for r in &pf.refs {
        if let Some(enc) = enclosing_def(&pf.defs, r.line) {
            let enc_id = stable_id(&pf.rel_path, &enc.name, &enc.kind);
            store.upsert_edge(&enc_id, &format!("unresolved:{}", r.name), &EdgeKind::Reference, 0.5, None).ok();
        }
    }
}

fn enclosing_def(defs: &[RawDef], line: u32) -> Option<&RawDef> {
    defs.iter()
        .filter(|d| d.start_line <= line && d.end_line >= line)
        .min_by_key(|d| d.end_line - d.start_line)
}

pub fn stable_id(file: &str, name: &str, kind: &SymbolKind) -> String {
    let mut h = DefaultHasher::new();
    file.hash(&mut h);
    name.hash(&mut h);
    kind.to_string().hash(&mut h);
    format!("{:016x}", h.finish())
}

fn compute_complexity(tree: &tree_sitter::Tree, source: &[u8], start: u32, end: u32) -> u32 {
    let branch_kinds = [
        "if_statement",
        "if_expression",
        "for_statement",
        "while_statement",
        "for_in_statement",
        "match_expression",
        "switch_statement",
        "case",
        "catch_clause",
        "binary_expression",
    ];

    let mut count = 0u32;
    let mut max_depth = 0u32;
    let mut cursor = tree.root_node().walk();

    #[allow(clippy::too_many_arguments)]
    fn visit(
        cursor: &mut tree_sitter::TreeCursor<'_>,
        _source: &[u8],
        start: u32,
        end: u32,
        branch_kinds: &[&str],
        count: &mut u32,
        max_depth: &mut u32,
        depth: u32,
    ) {
        let node = cursor.node();
        let node_line = node.start_position().row as u32 + 1;
        if node_line >= start && node_line <= end && branch_kinds.contains(&node.kind()) {
            *count += 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
        }
        if cursor.goto_first_child() {
            visit(
                cursor,
                _source,
                start,
                end,
                branch_kinds,
                count,
                max_depth,
                depth + 1,
            );
            while cursor.goto_next_sibling() {
                visit(
                    cursor,
                    _source,
                    start,
                    end,
                    branch_kinds,
                    count,
                    max_depth,
                    depth + 1,
                );
            }
            cursor.goto_parent();
        }
    }

    visit(
        &mut cursor,
        source,
        start,
        end,
        &branch_kinds,
        &mut count,
        &mut max_depth,
        0,
    );

    count + max_depth
}

#[derive(Debug)]
struct RawDef {
    name: String,
    kind: SymbolKind,
    start_line: u32,
    end_line: u32,
    complexity: u32,
}

#[derive(Debug)]
struct RawCall {
    callee: String,
    receiver: Option<String>,
    line: u32,
}

#[derive(Debug)]
struct RawRef {
    name: String,
    line: u32,
}

#[derive(Debug)]
struct RawPattern {
    kind: String,
    line: u32,
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
        assert!(
            la.patterns.contains(&"lock".to_string()),
            "missing lock, got {:?}",
            la.patterns
        );
        assert!(
            la.patterns.contains(&"await".to_string()),
            "missing await, got {:?}",
            la.patterns
        );

        assert!(loopy.is_some(), "has_loop not found");
        let lp = loopy.unwrap();
        assert!(
            lp.patterns.contains(&"loop".to_string()),
            "missing loop, got {:?}",
            lp.patterns
        );
    }
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
    let alpha = syms
        .iter()
        .find(|s| s.name == "alpha")
        .expect("alpha not found");
    let mut stmt = store
        .conn
        .prepare("SELECT to_id FROM edges WHERE from_id=?1 AND kind='calls'")
        .unwrap();
    let callee_ids: Vec<String> = stmt
        .query_map([&alpha.id], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(
        !callee_ids.is_empty(),
        "alpha should have unresolved call edges; got none"
    );
    assert!(
        callee_ids.iter().any(|id| id == "unresolved:beta"),
        "alpha must call beta; callee_ids={callee_ids:?}"
    );
}
