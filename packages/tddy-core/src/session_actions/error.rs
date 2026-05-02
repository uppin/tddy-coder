//! Errors for session action manifests and invocation helpers.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionActionsError {
    #[error("session actions: I/O ({0})")]
    Io(#[from] std::io::Error),

    #[error("session actions: invalid manifest YAML ({0})")]
    ManifestYaml(#[from] serde_yaml::Error),

    #[error("session actions: missing actions directory under {0}")]
    MissingActionsDir(PathBuf),

    #[error("session actions: {0}")]
    ArgumentsViolateSchema(String),

    #[error("session actions: invalid manifest input_schema shape ({0})")]
    InvalidSchemaShape(String),

    #[error(
        "session actions: invalid path binding ({purpose}): `{}` is outside session tree or declared repo",
        path
    )]
    PathOutsideAllowlist { path: String, purpose: &'static str },

    #[error("session actions: invalid path ({purpose}): {reason}")]
    PathTraversalAttempt {
        purpose: &'static str,
        reason: String,
    },

    #[error("session actions: host architecture `{requested}` does not match this host ({host})")]
    ArchitectureMismatch { requested: String, host: String },

    #[error(
        "session actions: manifest `command` must name a non-empty program as the first element"
    )]
    EmptyCommand,

    #[error("session actions: could not parse cargo-style test totals from command output")]
    TestSummaryParseFailed,

    #[error("session actions: unknown action id `{0}`")]
    UnknownActionId(String),

    #[error("session actions: invalid `--data` JSON ({0})")]
    InvalidInvokeJson(String),

    #[error("session actions: changeset read failed ({0})")]
    ChangesetRead(String),

    #[error("session actions: manifest program `{program}` ({detail})")]
    CommandSpawn { program: String, detail: String },
}
