use crate::{
    language::lang_config,
    queries::{DEF_FUNCTION, DEF_METHOD, DEF_TYPE, IMPORT, REF_CALL, REF_CALL_RECEIVER},
};
use bs_core::{EdgeKind, LangId, Result, Store, Symbol, SymbolKind};
use ignore::WalkBuilder;
use streaming_iterator::StreamingIterator;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Default)]
pub struct ExtractionResult {
    pub files_processed: usize,
    pub symbols_found: usize,
    pub files_skipped: usize,
}

pub fn extract_repo(
    store: &Store,
    repo_root: &Path,
    grammar_path: Option<&Path>,
) -> Result<ExtractionResult> {
    let _ = grammar_path; // dynamic loading reserved for later

    let result = Arc::new(Mutex::new(ExtractionResult::default()));

    let files: Vec<PathBuf> = WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();

    for file_path in files {
        let lang = LangId::from_path(&file_path);
        if matches!(lang, LangId::Unknown | LangId::Hcl | LangId::Yaml) {
            let mut r = result.lock().unwrap();
            r.files_skipped += 1;
            continue;
        }

        let rel = file_path
            .strip_prefix(repo_root)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .into_owned();

        match extract_file(store, &file_path, &rel, &lang) {
            Ok(n) => {
                let mut r = result.lock().unwrap();
                r.files_processed += 1;
                r.symbols_found += n;
            }
            Err(_) => {
                let mut r = result.lock().unwrap();
                r.files_skipped += 1;
            }
        }
    }

    Ok(Arc::try_unwrap(result).unwrap().into_inner().unwrap())
}

pub fn extract_file(
    store: &Store,
    abs_path: &Path,
    rel_path: &str,
    lang: &LangId,
) -> Result<usize> {
    let cfg = match lang_config(lang) {
        Some(c) => c,
        None => return Ok(0),
    };

    let source = std::fs::read(abs_path)?;
    let loc = source.iter().filter(|&&b| b == b'\n').count() as u32 + 1;

    let file_id = store.upsert_file(rel_path, lang, loc)?;

    let mut parser = Parser::new();
    parser
        .set_language(&cfg.ts_language)
        .map_err(|e| bs_core::Error::Parse(e.to_string()))?;

    let tree = match parser.parse(&source, None) {
        Some(t) => t,
        None => return Ok(0), // partial parse OK, just skip
    };

    let query = match Query::new(&cfg.ts_language, cfg.query_source) {
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

    let mut symbols_added = 0usize;

    // Two-pass: first collect defs, then calls/imports
    let mut defs: Vec<RawDef> = Vec::new();
    let mut calls: Vec<RawCall> = Vec::new();
    let mut imports: Vec<String> = Vec::new();

    while let Some(m) = matches.next() {
        for cap in m.captures {
            let cap_name = &cap_names[cap.index as usize];
            let node = cap.node;
            let text = node.utf8_text(&source).unwrap_or("").trim().to_string();
            let start_line = node.start_position().row as u32 + 1;
            let end_line = node.end_position().row as u32 + 1;

            if cap_name == DEF_FUNCTION || cap_name == DEF_METHOD || cap_name == DEF_TYPE {
                let kind = match cap_name.as_str() {
                    DEF_FUNCTION => SymbolKind::Function,
                    DEF_METHOD => SymbolKind::Method,
                    _ => SymbolKind::Type,
                };
                defs.push(RawDef {
                    name: text,
                    kind,
                    start_line,
                    end_line,
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
            } else if cap_name == IMPORT {
                imports.push(text);
            }
        }
    }

    // Upsert symbols
    for def in &defs {
        let qualified = format!("{}:{}", rel_path, def.name);
        let id = stable_id(rel_path, &def.name, &def.kind);
        let sym = Symbol {
            id: id.clone(),
            kind: def.kind.clone(),
            name: def.name.clone(),
            qualified,
            file: PathBuf::from(rel_path),
            span: (def.start_line, def.end_line),
            lang: lang.clone(),
            churn: 0,
            age_days: 0,
            loc: def.end_line.saturating_sub(def.start_line) + 1,
            complexity: compute_complexity(&tree, &source, def.start_line, def.end_line),
            hotspot: 0.0,
        };
        store.upsert_symbol(&sym)?;
        symbols_added += 1;

        // contains edge: file -> symbol
        let file_sym_id = format!("file:{}", rel_path);
        let _ = store.upsert_edge(&file_sym_id, &id, &EdgeKind::Contains, 1.0, None);
    }

    // Upsert import edges
    for import in &imports {
        let from_id = stable_id(rel_path, rel_path, &SymbolKind::File);
        let to_id = format!("import:{}", import);
        let _ = store.upsert_edge(&from_id, &to_id, &EdgeKind::Imports, 1.0, None);
    }

    // Upsert call refs (unresolved — bs-link will resolve them)
    for call in &calls {
        let callee_id = format!("unresolved:{}", call.callee);
        // Find enclosing def by line
        if let Some(enc) = enclosing_def(&defs, call.line) {
            let enc_id = stable_id(rel_path, &enc.name, &enc.kind);
            let _ = store.upsert_edge(&enc_id, &callee_id, &EdgeKind::Calls, 0.3, None);
        }
    }

    let _ = file_id;
    Ok(symbols_added)
}

fn enclosing_def<'a>(defs: &'a [RawDef], line: u32) -> Option<&'a RawDef> {
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
        "if_statement", "if_expression", "for_statement", "while_statement",
        "for_in_statement", "match_expression", "switch_statement",
        "case", "catch_clause", "binary_expression",
    ];

    let mut count = 0u32;
    let mut max_depth = 0u32;
    let mut cursor = tree.root_node().walk();

    fn visit(
        cursor: &mut tree_sitter::TreeCursor<'_>,
        source: &[u8],
        start: u32,
        end: u32,
        branch_kinds: &[&str],
        count: &mut u32,
        max_depth: &mut u32,
        depth: u32,
    ) {
        let node = cursor.node();
        let node_line = node.start_position().row as u32 + 1;
        if node_line >= start && node_line <= end {
            if branch_kinds.contains(&node.kind()) {
                *count += 1;
                if depth > *max_depth {
                    *max_depth = depth;
                }
            }
        }
        if cursor.goto_first_child() {
            visit(cursor, source, start, end, branch_kinds, count, max_depth, depth + 1);
            while cursor.goto_next_sibling() {
                visit(cursor, source, start, end, branch_kinds, count, max_depth, depth + 1);
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
}

#[derive(Debug)]
struct RawCall {
    callee: String,
    receiver: Option<String>,
    line: u32,
}
