//! Workflow layer: session artifact roots and manifest-driven layout paths.

pub mod artifact_paths;

pub use artifact_paths::{
    canonical_artifact_write_path, read_session_artifact_utf8,
    read_session_artifact_utf8_or_placeholder, resolve_existing_session_artifact,
    session_artifacts_root, SESSION_ARTIFACT_READ_PLACEHOLDER,
};
