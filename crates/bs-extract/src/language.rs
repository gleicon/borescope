use bs_core::LangId;
use tree_sitter::Language;

pub struct LangConfig {
    pub lang: LangId,
    pub ts_language: Language,
    pub query_source: String,
}

/// Load config for `lang`, appending `.borescope/queries/<lang>.scm` if present.
pub fn lang_config(
    lang: &LangId,
    query_override_dir: Option<&std::path::Path>,
) -> Option<LangConfig> {
    let mut cfg = builtin_config(lang)?;
    if let Some(dir) = query_override_dir {
        let override_path = dir.join(format!("{}.scm", lang));
        if let Ok(extra) = std::fs::read_to_string(&override_path) {
            if !extra.trim().is_empty() {
                cfg.query_source.push('\n');
                cfg.query_source.push_str(&extra);
            }
        }
    }
    Some(cfg)
}

/// Load a grammar from --grammar-path directory.
/// Expects: <dir>/<lang>.so (Linux) or <dir>/<lang>.dylib (macOS)
/// and <dir>/<lang>.scm for the query pack.
/// The .so must export `tree_sitter_<lang>` returning a Language-compatible pointer.
pub fn load_dynamic_grammar(dir: &std::path::Path, lang_name: &str) -> Option<LangConfig> {
    let lib_ext = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let lib_path = dir.join(format!("{}.{}", lang_name, lib_ext));
    let query_path = dir.join(format!("{}.scm", lang_name));

    if !lib_path.exists() || !query_path.exists() {
        return None;
    }

    let query_source = std::fs::read_to_string(&query_path).ok()?;

    // Safety: we trust the user-provided grammar path
    unsafe {
        use libloading::{Library, Symbol};
        let lib = Library::new(&lib_path).ok()?;
        let func: Symbol<unsafe extern "C" fn() -> Language> = lib
            .get(format!("tree_sitter_{}", lang_name).as_bytes())
            .ok()?;
        let language = func();
        // Leak the library so the Language pointer stays valid for the process lifetime
        std::mem::forget(lib);
        Some(LangConfig {
            lang: LangId::Unknown,
            ts_language: language,
            query_source,
        })
    }
}

fn builtin_config(lang: &LangId) -> Option<LangConfig> {
    match lang {
        LangId::Go => Some(LangConfig {
            lang: LangId::Go,
            ts_language: tree_sitter_go::LANGUAGE.into(),
            query_source: include_str!("../queries/go.scm").to_string(),
        }),
        LangId::Rust => Some(LangConfig {
            lang: LangId::Rust,
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            query_source: include_str!("../queries/rust.scm").to_string(),
        }),
        LangId::Python => Some(LangConfig {
            lang: LangId::Python,
            ts_language: tree_sitter_python::LANGUAGE.into(),
            query_source: include_str!("../queries/python.scm").to_string(),
        }),
        LangId::JavaScript => Some(LangConfig {
            lang: LangId::JavaScript,
            ts_language: tree_sitter_javascript::LANGUAGE.into(),
            query_source: include_str!("../queries/javascript.scm").to_string(),
        }),
        LangId::TypeScript => Some(LangConfig {
            lang: LangId::TypeScript,
            ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            query_source: include_str!("../queries/typescript.scm").to_string(),
        }),
        LangId::Java => Some(LangConfig {
            lang: LangId::Java,
            ts_language: tree_sitter_java::LANGUAGE.into(),
            query_source: include_str!("../queries/java.scm").to_string(),
        }),
        LangId::Ruby => Some(LangConfig {
            lang: LangId::Ruby,
            ts_language: tree_sitter_ruby::LANGUAGE.into(),
            query_source: include_str!("../queries/ruby.scm").to_string(),
        }),
        LangId::C => Some(LangConfig {
            lang: LangId::C,
            ts_language: tree_sitter_c::LANGUAGE.into(),
            query_source: include_str!("../queries/c.scm").to_string(),
        }),
        LangId::Cpp => Some(LangConfig {
            lang: LangId::Cpp,
            ts_language: tree_sitter_cpp::LANGUAGE.into(),
            query_source: include_str!("../queries/cpp.scm").to_string(),
        }),
        LangId::Bash => Some(LangConfig {
            lang: LangId::Bash,
            ts_language: tree_sitter_bash::LANGUAGE.into(),
            query_source: include_str!("../queries/bash.scm").to_string(),
        }),
        _ => None,
    }
}
