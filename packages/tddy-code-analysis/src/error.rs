//! Analysis errors.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AnalysisError>;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("coverage artifacts missing at {path}: run `tddy-tools analyze coverage` first")]
    MissingCoverage { path: String },
    #[error("llvm tool not found: {tool} — install llvm-tools-preview in the dev shell")]
    MissingLlvmTool { tool: String },
    #[error("cargo failed: {0}")]
    Cargo(String),
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
