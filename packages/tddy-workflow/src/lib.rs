//! Workflow layer: artifact roots, primary approval paths, and manifest-driven layout.

pub mod artifact_paths;

pub use artifact_paths::{
    primary_planning_artifact_path_for_basename, read_primary_planning_document_utf8,
    read_primary_planning_document_utf8_or_placeholder, resolve_existing_primary_planning_document,
    session_artifacts_root, PRIMARY_PLANNING_DOCUMENT_READ_PLACEHOLDER,
};
