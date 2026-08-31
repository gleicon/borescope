use super::{ParsedFile, RawDef, RawPattern};
use bs_core::{EdgeKind, Result, Store, Symbol, SymbolKind};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

pub(super) fn write_parsed(store: &Store, pf: ParsedFile) -> Result<usize> {
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
        store
            .upsert_edge(&from_id, &format!("import:{}", import), &EdgeKind::Imports, 1.0, None)
            .ok();
    }
}

fn write_calls(store: &Store, pf: &ParsedFile) {
    for call in &pf.calls {
        if let Some(enc) = enclosing_def(&pf.defs, call.line) {
            let enc_id = stable_id(&pf.rel_path, &enc.name, &enc.kind);
            store
                .upsert_edge(&enc_id, &format!("unresolved:{}", call.callee), &EdgeKind::Calls, 0.3, None)
                .ok();
        }
    }
}

fn write_refs(store: &Store, pf: &ParsedFile) {
    for r in &pf.refs {
        if let Some(enc) = enclosing_def(&pf.defs, r.line) {
            let enc_id = stable_id(&pf.rel_path, &enc.name, &enc.kind);
            store
                .upsert_edge(&enc_id, &format!("unresolved:{}", r.name), &EdgeKind::Reference, 0.5, None)
                .ok();
        }
    }
}

fn enclosing_def(defs: &[RawDef], line: u32) -> Option<&RawDef> {
    defs.iter()
        .filter(|d| d.start_line <= line && d.end_line >= line)
        .min_by_key(|d| d.end_line - d.start_line)
}

pub(super) fn stable_id(file: &str, name: &str, kind: &SymbolKind) -> String {
    let mut h = DefaultHasher::new();
    file.hash(&mut h);
    name.hash(&mut h);
    kind.to_string().hash(&mut h);
    format!("{:016x}", h.finish())
}
