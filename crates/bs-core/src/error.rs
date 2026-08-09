use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("git: {0}")]
    Git(String),

    #[error("parse: {0}")]
    Parse(String),

    #[error("no index found — run `borescope index` first")]
    NoIndex,

    #[error("ambiguous target '{0}' — {1} candidates")]
    AmbiguousTarget(String, usize),

    #[error("unknown target: {0}")]
    UnknownTarget(String),

    #[error("grammar unavailable for {0}")]
    GrammarUnavailable(String),

    #[error("{0}")]
    Other(String),
}
