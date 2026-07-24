//! Recipe-manifest–derived context documents for a session: the *list* of relevant planning
//! docs (surfaced on `SessionEntry` and to child "Start session" prompts) and an allowlisted,
//! canonicalize-and-contained reader for their *contents*, rooted at `session_artifacts_root`.
//!
//! The allowlist is the recipe's [`SessionArtifactManifest::known_artifacts`] — nothing else under
//! the session directory is readable through this surface (mirrors the guard shape in
//! [`crate::session_workflow_files`]). Contents live under `session_dir/artifacts/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_rpc::Status;
use tddy_workflow::session_artifacts_root;
use tddy_workflow_recipes::{workflow_recipe_and_manifest_from_cli_name, SessionArtifactManifest};

/// One recipe-manifest–derived planning document for a session: its manifest `key`, on-disk
/// `basename`, absolute artifacts/ `path` (not canonicalized — the file may not exist), a human
/// `description`, and whether it currently `exists` on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDoc {
    pub key: String,
    pub basename: String,
    pub path: PathBuf,
    pub description: String,
    pub exists: bool,
}

/// Resolve a recipe's [`SessionArtifactManifest`], or `None` for a blank or unknown recipe.
///
/// Docs are surfaced only when the recipe is known — a blank name never falls back to a default
/// recipe (the resolver maps `""` to tdd, so the blank case is guarded here before calling it).
fn manifest_for_recipe(recipe_name: &str) -> Option<Arc<dyn SessionArtifactManifest>> {
    if recipe_name.trim().is_empty() {
        log::debug!("session_context_docs: blank recipe name — no context docs");
        return None;
    }
    match workflow_recipe_and_manifest_from_cli_name(recipe_name) {
        Ok((_, manifest)) => Some(manifest),
        Err(e) => {
            log::debug!("session_context_docs: unknown recipe {recipe_name:?}: {e}");
            None
        }
    }
}

/// Enumerate the recipe manifest's context docs for a session, resolving each to an absolute
/// `artifacts/` path and reporting its on-disk existence and human description.
///
/// Returns an empty `Vec` for a blank or unknown recipe (docs are surfaced only when the recipe is
/// known).
pub fn context_docs_for_session(recipe_name: &str, session_dir: &Path) -> Vec<ContextDoc> {
    let Some(manifest) = manifest_for_recipe(recipe_name) else {
        return Vec::new();
    };

    let artifacts_root = session_artifacts_root(session_dir);
    let descriptions = manifest.artifact_doc_descriptions();

    let docs: Vec<ContextDoc> = manifest
        .known_artifacts()
        .iter()
        .map(|(key, basename)| {
            let path = artifacts_root.join(basename);
            let exists = path.is_file();
            ContextDoc {
                key: (*key).to_string(),
                basename: (*basename).to_string(),
                path,
                description: descriptions.get(key).copied().unwrap_or("").to_string(),
                exists,
            }
        })
        .collect();

    log::debug!(
        "context_docs_for_session: recipe={recipe_name:?} listed {} doc(s) for {}",
        docs.len(),
        session_dir.display()
    );
    docs
}

/// Reads an allowlisted context doc as UTF-8 text from a session's `artifacts/` directory.
///
/// The allowlist is the recipe manifest's [`SessionArtifactManifest::known_artifacts`] basenames.
/// A basename outside the allowlist, or one containing traversal/separator segments, is refused
/// with [`Status::permission_denied`]; the resolved path must remain under the canonical artifacts
/// root.
pub fn read_session_context_doc_utf8(
    recipe_name: &str,
    session_dir: &Path,
    basename: &str,
) -> Result<String, Status> {
    log::debug!(
        "read_session_context_doc_utf8: recipe={recipe_name:?} session_dir={} basename={basename:?}",
        session_dir.display()
    );

    if basename.contains("..") || basename.contains('/') || basename.contains('\\') {
        log::debug!("read_session_context_doc_utf8: rejected unsafe basename {basename:?}");
        return Err(Status::permission_denied(
            "basename must be a single path segment without traversal",
        ));
    }

    let Some(manifest) = manifest_for_recipe(recipe_name) else {
        return Err(Status::permission_denied(
            "no context docs are available for this recipe",
        ));
    };

    let allowlisted = manifest
        .known_artifacts()
        .iter()
        .any(|(_, name)| *name == basename);
    if !allowlisted {
        log::debug!("read_session_context_doc_utf8: basename not allowlisted: {basename:?}");
        return Err(Status::permission_denied(
            "basename is not a context doc for this recipe",
        ));
    }

    let artifacts_root = session_artifacts_root(session_dir);
    let canonical_root = artifacts_root.canonicalize().map_err(|e| {
        log::debug!("read_session_context_doc_utf8: canonicalize artifacts root failed: {e}");
        Status::failed_precondition("session artifacts directory is not accessible")
    })?;

    let joined = artifacts_root.join(basename);
    let canonical_file = joined.canonicalize().map_err(|e| {
        log::debug!("read_session_context_doc_utf8: canonicalize {joined:?} failed: {e}");
        Status::not_found("context doc not found")
    })?;

    if !canonical_file.starts_with(&canonical_root) {
        log::warn!(
            "read_session_context_doc_utf8: rejected path outside artifacts root: {canonical_file:?} (root {canonical_root:?})"
        );
        return Err(Status::permission_denied(
            "resolved path escapes the session artifacts directory",
        ));
    }

    let meta = std::fs::metadata(&canonical_file).map_err(|e| {
        log::debug!("read_session_context_doc_utf8: metadata failed: {e}");
        Status::not_found("context doc not found")
    })?;
    if !meta.is_file() {
        return Err(Status::failed_precondition("not a regular file"));
    }

    let content = std::fs::read_to_string(&canonical_file).map_err(|e| {
        log::error!("read_session_context_doc_utf8: read_to_string {canonical_file:?} failed: {e}");
        Status::internal(format!("failed to read context doc: {e}"))
    })?;

    log::info!(
        "read_session_context_doc_utf8: read {} UTF-8 chars from {basename:?}",
        content.len()
    );
    Ok(content)
}

#[cfg(test)]
mod tests {
    // The production API these tests define. Until `/green` implements it, these imports are
    // unresolved and the crate's test build fails — the accepted red signal for a not-yet-written
    // module (mirrors the pr-stack `exploration` field red in tddy-workflow-recipes).
    use super::{context_docs_for_session, read_session_context_doc_utf8, ContextDoc};

    use std::fs;
    use std::path::{Path, PathBuf};

    use tddy_rpc::{Code, Status};

    // ---- fluent helpers -------------------------------------------------------------------

    /// Creates `session_dir/artifacts/` and returns its path.
    fn artifacts_dir_in(session_dir: &Path) -> PathBuf {
        let artifacts = session_dir.join("artifacts");
        fs::create_dir_all(&artifacts).expect("create artifacts dir");
        artifacts
    }

    /// Finds the context doc with the given manifest `key`, failing with a clear message otherwise.
    fn find_doc<'a>(docs: &'a [ContextDoc], key: &str) -> &'a ContextDoc {
        docs.iter().find(|d| d.key == key).unwrap_or_else(|| {
            let keys: Vec<&str> = docs.iter().map(|d| d.key.as_str()).collect();
            panic!("expected a context doc with key {key:?}, got keys {keys:?}");
        })
    }

    trait ContextDocAssertions {
        fn assert_basename(&self, expected: &str) -> &Self;
        fn assert_path(&self, expected: &Path) -> &Self;
        fn assert_exists(&self, expected: bool) -> &Self;
        fn assert_has_description(&self) -> &Self;
    }

    impl ContextDocAssertions for ContextDoc {
        fn assert_basename(&self, expected: &str) -> &Self {
            assert_eq!(
                self.basename, expected,
                "context doc {:?} basename",
                self.key
            );
            self
        }

        fn assert_path(&self, expected: &Path) -> &Self {
            assert_eq!(self.path, expected, "context doc {:?} path", self.key);
            self
        }

        fn assert_exists(&self, expected: bool) -> &Self {
            assert_eq!(
                self.exists, expected,
                "context doc {:?} on-disk existence",
                self.key
            );
            self
        }

        // Exact wording is a copy decision finalized in green; at this layer the contract is only
        // that every listed doc carries a non-empty human description.
        fn assert_has_description(&self) -> &Self {
            assert!(
                !self.description.trim().is_empty(),
                "context doc {:?} must carry a non-empty description",
                self.key
            );
            self
        }
    }

    /// Asserts a read was refused with `PermissionDenied` (the contract for both a non-manifest
    /// basename and a traversal attempt).
    fn assert_permission_denied<T>(result: Result<T, Status>) {
        let status = result
            .err()
            .expect("expected a PermissionDenied Status, but the read succeeded");
        assert_eq!(
            status.code,
            Code::PermissionDenied,
            "expected PermissionDenied, got {:?} ({})",
            status.code,
            status.message
        );
    }

    // ---- tests ----------------------------------------------------------------------------

    #[test]
    fn context_docs_for_a_pr_stack_session_lists_manifest_docs_with_descriptions_and_absolute_paths(
    ) {
        // Given — a pr-stack session whose artifacts/ holds the exploration doc and the stack-plan
        // YAML, but not the rendered pr-stack-plan.md
        let session = tempfile::tempdir().unwrap();
        let artifacts = artifacts_dir_in(session.path());
        fs::write(
            artifacts.join("exploration.md"),
            "# Exploration\n- src/lib.rs:1\n",
        )
        .unwrap();
        fs::write(artifacts.join("stack-plan.yaml"), "version: 1\nnodes: []\n").unwrap();

        // When — enumerating the recipe's context docs for that session
        let docs = context_docs_for_session("pr-stack", session.path());

        // Then — each manifest doc is listed with its basename, a human description, an absolute
        // artifacts/ path, and an existence flag reflecting what is on disk
        find_doc(&docs, "exploration")
            .assert_basename("exploration.md")
            .assert_path(&artifacts.join("exploration.md"))
            .assert_exists(true)
            .assert_has_description();

        find_doc(&docs, "stack_plan")
            .assert_basename("stack-plan.yaml")
            .assert_path(&artifacts.join("stack-plan.yaml"))
            .assert_exists(true)
            .assert_has_description();

        find_doc(&docs, "stack_plan_md")
            .assert_basename("pr-stack-plan.md")
            .assert_path(&artifacts.join("pr-stack-plan.md"))
            .assert_exists(false)
            .assert_has_description();
    }

    #[test]
    fn reading_an_allowlisted_context_doc_returns_its_utf8_contents() {
        // Given — the exploration doc on disk under artifacts/
        let session = tempfile::tempdir().unwrap();
        let artifacts = artifacts_dir_in(session.path());
        let golden = "# Exploration\n- src/lib.rs:1 entry point\n";
        fs::write(artifacts.join("exploration.md"), golden).unwrap();

        // When — reading it by its allowlisted basename
        let content = read_session_context_doc_utf8("pr-stack", session.path(), "exploration.md")
            .expect("an allowlisted context doc must be readable");

        // Then — the exact bytes come back
        assert_eq!(content, golden);
    }

    #[test]
    fn reading_a_basename_not_in_the_recipe_manifest_is_permission_denied() {
        // Given — a sensitive file dropped in artifacts/ that the manifest never lists
        let session = tempfile::tempdir().unwrap();
        let artifacts = artifacts_dir_in(session.path());
        fs::write(artifacts.join(".env"), "SECRET=x\n").unwrap();

        // When — attempting to read it through the context-doc surface
        let result = read_session_context_doc_utf8("pr-stack", session.path(), ".env");

        // Then — the read is refused
        assert_permission_denied(result);
    }

    #[test]
    fn reading_a_traversal_path_is_permission_denied() {
        // Given — a pr-stack session with an artifacts/ dir
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());

        // When — a traversal basename tries to escape the artifacts root
        let result = read_session_context_doc_utf8("pr-stack", session.path(), "../../secret");

        // Then — the read is refused
        assert_permission_denied(result);
    }
}
