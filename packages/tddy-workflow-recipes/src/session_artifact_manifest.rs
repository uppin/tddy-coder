//! Session artifact manifest — lives in recipes, not in `tddy-core`.

use std::collections::BTreeMap;

/// Filenames and keys for session artifacts for a workflow recipe (TDD, bug-fix, …).
pub trait SessionArtifactManifest: Send + Sync {
    fn known_artifacts(&self) -> &[(&'static str, &'static str)];

    fn default_artifacts(&self) -> BTreeMap<String, String>;

    /// Basename of the primary human-edited session document (e.g. PRD) under `session_dir/artifacts/`, if any.
    fn primary_document_basename(&self) -> Option<String> {
        self.default_artifacts().get("prd").cloned().or_else(|| {
            self.known_artifacts()
                .iter()
                .find(|(k, _)| *k == "prd")
                .map(|(_, name)| (*name).to_string())
        })
    }

    fn context_header_filenames(&self) -> Vec<&'static str> {
        self.known_artifacts()
            .iter()
            .map(|(_, name)| *name)
            .collect()
    }
}
