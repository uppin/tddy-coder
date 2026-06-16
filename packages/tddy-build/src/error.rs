//! Error type for tddy-build.

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("yaml parse error: {0}")]
    Yaml(String),

    #[error("manifest error: {0}")]
    Manifest(String),

    #[error("build graph cycle detected: {0}")]
    Cycle(String),

    #[error("unknown target: {0}")]
    UnknownTarget(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("action execution failed: {0}")]
    Exec(String),
}
