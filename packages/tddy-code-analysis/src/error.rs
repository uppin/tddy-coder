//! Analysis errors.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AnalysisError>;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("coverage artifacts missing at {path}: run `tddy-tools analyze coverage` first")]
    MissingCoverage { path: String },
    #[error("llvm tool not found: {tool} — install llvm-tools-preview in the dev shell")]
    MissingLlvmTool { tool: String },
    /// The path to analyse names no crate. Its own variant rather than a [`Self::Message`],
    /// because it is the one refusal here that is about *what was asked for* rather than about the
    /// state of the tree or of this host — a distinction a caller acts on, and one that is lost
    /// the moment it is prose.
    #[error("no Cargo.toml at {path}: name a crate directory or its manifest")]
    NotACrate { path: String },
    #[error("cargo failed: {0}")]
    Cargo(String),
    /// A refusal with no class of its own. Reaching for this leaves a caller unable to tell a
    /// request it should change from a state it should repair, so anything a caller could act on
    /// differently belongs in a variant instead.
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Syn(#[from] syn::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
