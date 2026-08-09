use bs_core::LangId;
use tree_sitter::Language;

pub struct LangConfig {
    pub lang: LangId,
    pub ts_language: Language,
    pub query_source: &'static str,
}

pub fn lang_config(lang: &LangId) -> Option<LangConfig> {
    match lang {
        LangId::Go => Some(LangConfig {
            lang: LangId::Go,
            ts_language: tree_sitter_go::LANGUAGE.into(),
            query_source: include_str!("../queries/go.scm"),
        }),
        LangId::Rust => Some(LangConfig {
            lang: LangId::Rust,
            ts_language: tree_sitter_rust::LANGUAGE.into(),
            query_source: include_str!("../queries/rust.scm"),
        }),
        LangId::Python => Some(LangConfig {
            lang: LangId::Python,
            ts_language: tree_sitter_python::LANGUAGE.into(),
            query_source: include_str!("../queries/python.scm"),
        }),
        LangId::JavaScript => Some(LangConfig {
            lang: LangId::JavaScript,
            ts_language: tree_sitter_javascript::LANGUAGE.into(),
            query_source: include_str!("../queries/javascript.scm"),
        }),
        LangId::TypeScript => Some(LangConfig {
            lang: LangId::TypeScript,
            ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            query_source: include_str!("../queries/typescript.scm"),
        }),
        LangId::Java => Some(LangConfig {
            lang: LangId::Java,
            ts_language: tree_sitter_java::LANGUAGE.into(),
            query_source: include_str!("../queries/java.scm"),
        }),
        LangId::Ruby => Some(LangConfig {
            lang: LangId::Ruby,
            ts_language: tree_sitter_ruby::LANGUAGE.into(),
            query_source: include_str!("../queries/ruby.scm"),
        }),
        LangId::C => Some(LangConfig {
            lang: LangId::C,
            ts_language: tree_sitter_c::LANGUAGE.into(),
            query_source: include_str!("../queries/c.scm"),
        }),
        LangId::Cpp => Some(LangConfig {
            lang: LangId::Cpp,
            ts_language: tree_sitter_cpp::LANGUAGE.into(),
            query_source: include_str!("../queries/cpp.scm"),
        }),
        LangId::Bash => Some(LangConfig {
            lang: LangId::Bash,
            ts_language: tree_sitter_bash::LANGUAGE.into(),
            query_source: include_str!("../queries/bash.scm"),
        }),
        _ => None,
    }
}
