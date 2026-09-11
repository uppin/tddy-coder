//! Context documents for a session: the *list* of relevant planning docs (surfaced on
//! `SessionEntry` and to child "Start session" prompts) and an allowlisted,
//! canonicalize-and-contained reader for their *contents*, rooted at `session_artifacts_root`.
//!
//! The list has two kinds (see `docs/ft/coder/session-attachments.md`): the recipe manifest's own
//! artifacts under `session_dir/artifacts/`, followed by the user-attached documents under
//! `artifacts/attachments/` that [`crate::session_attachments`] stores. Attachments are
//! recipe-independent — a session with a blank or unknown recipe still lists them.
//!
//! Manifest rows come from two sources, because [`SessionArtifactManifest::known_artifacts`]
//! returns `&'static` basenames and so cannot enumerate the N per-PR documents a PR-stack
//! orchestrator writes under `artifacts/prs/<node_id>/`. Those are discovered by scanning, exactly
//! as attachment rows are, and are `Manifest` rather than `Attachment`: the docs pass authored
//! them, no operator attached them (`docs/ft/coder/pr-stack-docs.md` § Listing per-PR documents).
//!
//! The *reader* covers manifest docs only: its allowlist is the recipe's
//! [`SessionArtifactManifest::known_artifacts`], and nothing else under the session directory is
//! readable through this surface (mirrors the guard shape in [`crate::session_workflow_files`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_rpc::Status;
use tddy_workflow::{session_artifacts_root, SESSION_ATTACHMENTS_SUBDIR};
use tddy_workflow_recipes::pr_stack::{
    NODE_CHANGESET_BASENAME, NODE_DOCS_SUBDIR, NODE_PRD_BASENAME,
};
use tddy_workflow_recipes::{workflow_recipe_and_manifest_from_cli_name, SessionArtifactManifest};

use crate::session_attachments::list_session_attachments;

/// Whether a context doc is recipe-owned (from the session's recipe manifest) or user-attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextDocKind {
    Manifest,
    Attachment,
}

/// Human description carried by every attachment row (attachments have no recipe-authored copy).
pub const ATTACHMENT_DOC_DESCRIPTION: &str = "Attached document";

/// One planning document for a session: its `key` (manifest key for a manifest doc, basename for an
/// attachment), on-disk `basename`, absolute `path` (not canonicalized — a manifest doc's file may
/// not exist), its `relative_path` under the artifacts root, a human `description`, whether it
/// currently `exists` on disk, its `kind`, and its on-disk size in bytes (`0` when it does not
/// exist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDoc {
    pub key: String,
    pub basename: String,
    pub path: PathBuf,
    /// Address under the artifacts root, POSIX-separated: a stack-level artifact's own basename,
    /// `attachments/<basename>` for an attachment, `prs/<node_id>/<basename>` for a per-PR
    /// document. This is what a `HostDocumentRef` under `SESSION_ARTIFACT` carries, and it is the
    /// server's to state — a client cannot tell a nested document from a flat one.
    pub relative_path: String,
    pub description: String,
    pub exists: bool,
    pub kind: ContextDocKind,
    pub size_bytes: u64,
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

/// The per-PR documents a docs pass wrote under `artifacts/prs/<node_id>/`, in node-id order and,
/// within a node, its PRD before its changeset.
///
/// Discovered by scanning because [`SessionArtifactManifest::known_artifacts`] returns `&'static`
/// basenames and a stack has N nodes. Only the two authored basenames are surfaced: a node
/// directory is somewhere an agent can write, so listing whatever turns up there would let an
/// editor backup or a scratch note present itself as a recipe-authored document.
fn per_pr_context_docs(artifacts_root: &Path) -> Vec<ContextDoc> {
    let Ok(entries) = std::fs::read_dir(artifacts_root.join(NODE_DOCS_SUBDIR)) else {
        // No docs pass has run (or this recipe writes no per-PR documents at all).
        return Vec::new();
    };
    let mut node_ids: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    node_ids.sort();

    let mut docs: Vec<ContextDoc> = Vec::new();
    for node_id in node_ids {
        let authored = [
            (
                NODE_PRD_BASENAME,
                format!("PR {node_id}: what this PR delivers"),
            ),
            (
                NODE_CHANGESET_BASENAME,
                format!(
                    "PR {node_id}: responsibility, boundaries, dependencies and the draft-PR contract"
                ),
            ),
        ];
        for (basename, description) in authored {
            let relative_path = format!("{NODE_DOCS_SUBDIR}/{node_id}/{basename}");
            let path = artifacts_root.join(&relative_path);
            let Some(file_metadata) = std::fs::metadata(&path).ok().filter(|m| m.is_file()) else {
                continue;
            };
            docs.push(ContextDoc {
                // Keyed by path: a stack has one `PRD.md` per node, so the basename alone would
                // name several rows.
                key: relative_path.clone(),
                basename: basename.to_string(),
                path,
                relative_path,
                description,
                exists: true,
                kind: ContextDocKind::Manifest,
                size_bytes: file_metadata.len(),
            });
        }
    }
    docs
}

/// Enumerate a session's context docs: the recipe manifest's artifacts (in manifest order, each
/// resolved to an absolute `artifacts/` path with its on-disk existence, size and human
/// description), then the per-PR documents under `artifacts/prs/`, then the session's attachments
/// in basename order.
///
/// The manifest half is empty for a blank or unknown recipe (manifest docs are surfaced only when
/// the recipe is known); attachments are listed either way.
pub fn context_docs_for_session(recipe_name: &str, session_dir: &Path) -> Vec<ContextDoc> {
    let mut docs: Vec<ContextDoc> = Vec::new();

    if let Some(manifest) = manifest_for_recipe(recipe_name) {
        let artifacts_root = session_artifacts_root(session_dir);
        let descriptions = manifest.artifact_doc_descriptions();

        docs.extend(manifest.known_artifacts().iter().map(|(key, basename)| {
            let path = artifacts_root.join(basename);
            // One stat call answers both "is it there" and "how big is it".
            let file_metadata = std::fs::metadata(&path).ok().filter(|m| m.is_file());
            ContextDoc {
                key: (*key).to_string(),
                basename: (*basename).to_string(),
                path,
                // A stack-level artifact sits directly under the artifacts root.
                relative_path: (*basename).to_string(),
                description: descriptions.get(key).copied().unwrap_or("").to_string(),
                exists: file_metadata.is_some(),
                kind: ContextDocKind::Manifest,
                size_bytes: file_metadata.map_or(0, |m| m.len()),
            }
        }));

        // Not gated on the recipe name: a recipe that authors no per-PR documents has no `prs/`
        // directory, so the scan finds nothing rather than needing to be told not to look.
        docs.extend(per_pr_context_docs(&artifacts_root));
    }

    let manifest_doc_count = docs.len();

    docs.extend(
        list_session_attachments(session_dir)
            .into_iter()
            .map(|file| ContextDoc {
                key: file.basename.clone(),
                relative_path: format!("{SESSION_ATTACHMENTS_SUBDIR}/{}", file.basename),
                basename: file.basename,
                path: file.path,
                description: ATTACHMENT_DOC_DESCRIPTION.to_string(),
                // A listed attachment is on disk by construction.
                exists: true,
                kind: ContextDocKind::Attachment,
                size_bytes: file.size_bytes,
            }),
    );

    log::debug!(
        "context_docs_for_session: recipe={recipe_name:?} listed {} manifest doc(s) and {} attachment(s) for {}",
        manifest_doc_count,
        docs.len() - manifest_doc_count,
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
///
/// Attachments are deliberately **not** readable here, even though
/// [`context_docs_for_session`] lists them: an attachment may be an image or another binary, so a
/// UTF-8 reader is the wrong shape for it. An attachment basename is therefore refused like any
/// other non-manifest name; clients render attachments from the listing (name, size, path) until a
/// type-aware content fetch exists.
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
    use super::{
        context_docs_for_session, read_session_context_doc_utf8, ContextDoc, ContextDocKind,
        ATTACHMENT_DOC_DESCRIPTION,
    };
    use crate::session_attachments::copy_attachment_into_session;

    use std::fs;
    use std::path::{Path, PathBuf};

    use tddy_rpc::{Code, Status};
    use tddy_workflow::session_attachments_root;

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

    /// Attaches a document to the session through the production store.
    fn attach_document(session_dir: &Path, basename: &str, contents: &str) {
        let sources = tempfile::tempdir().expect("create sources dir");
        let source = sources.path().join(basename);
        fs::write(&source, contents).expect("write source file");
        copy_attachment_into_session(session_dir, &source, basename)
            .expect("attaching a document must succeed");
    }

    /// Finds the context doc of a given `kind` with the given `key`.
    fn find_doc_of_kind<'a>(
        docs: &'a [ContextDoc],
        kind: ContextDocKind,
        key: &str,
    ) -> &'a ContextDoc {
        docs.iter()
            .find(|d| d.kind == kind && d.key == key)
            .unwrap_or_else(|| {
                panic!(
                    "expected a {kind:?} context doc with key {key:?}, got {:?}",
                    kinds_and_basenames(docs)
                )
            })
    }

    /// `(kind, basename)` per doc, in listing order.
    fn kinds_and_basenames(docs: &[ContextDoc]) -> Vec<(ContextDocKind, String)> {
        docs.iter().map(|d| (d.kind, d.basename.clone())).collect()
    }

    trait ContextDocAssertions {
        fn assert_basename(&self, expected: &str) -> &Self;
        fn assert_path(&self, expected: &Path) -> &Self;
        fn assert_exists(&self, expected: bool) -> &Self;
        fn assert_has_description(&self) -> &Self;
        fn assert_description(&self, expected: &str) -> &Self;
        fn assert_kind(&self, expected: ContextDocKind) -> &Self;
        fn assert_size_bytes(&self, expected: u64) -> &Self;
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

        fn assert_description(&self, expected: &str) -> &Self {
            assert_eq!(
                self.description, expected,
                "context doc {:?} description",
                self.key
            );
            self
        }

        fn assert_kind(&self, expected: ContextDocKind) -> &Self {
            assert_eq!(self.kind, expected, "context doc {:?} kind", self.key);
            self
        }

        fn assert_size_bytes(&self, expected: u64) -> &Self {
            assert_eq!(
                self.size_bytes, expected,
                "context doc {:?} size_bytes",
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

    // ---- attachments ----------------------------------------------------------------------

    #[test]
    fn context_docs_list_the_manifest_docs_first_then_the_attachments() {
        // Given — a pr-stack session with one attached document
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());
        attach_document(session.path(), "notes.md", "meeting notes\n");

        // When
        let docs = context_docs_for_session("pr-stack", session.path());

        // Then — the recipe's manifest docs keep their order and precede the attachment
        assert_eq!(
            kinds_and_basenames(&docs),
            vec![
                (ContextDocKind::Manifest, "stack-plan.yaml".to_string()),
                (ContextDocKind::Manifest, "pr-stack-plan.md".to_string()),
                (ContextDocKind::Manifest, "stack-status.md".to_string()),
                (ContextDocKind::Manifest, "stack-status.json".to_string()),
                (ContextDocKind::Manifest, "exploration.md".to_string()),
                (ContextDocKind::Attachment, "notes.md".to_string()),
            ]
        );
    }

    #[test]
    fn an_attachment_context_doc_carries_its_basename_as_key_with_a_size_and_a_description() {
        // Given — a pr-stack session with one attached document
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());
        attach_document(session.path(), "notes.md", "meeting notes\n");

        // When
        let docs = context_docs_for_session("pr-stack", session.path());

        // Then — the attachment is keyed by its basename and describes a file that is on disk
        find_doc_of_kind(&docs, ContextDocKind::Attachment, "notes.md")
            .assert_kind(ContextDocKind::Attachment)
            .assert_basename("notes.md")
            .assert_path(&session_attachments_root(session.path()).join("notes.md"))
            .assert_exists(true)
            .assert_size_bytes("meeting notes\n".len() as u64)
            .assert_description(ATTACHMENT_DOC_DESCRIPTION);
    }

    #[test]
    fn context_docs_for_a_blank_recipe_list_only_the_attachments() {
        // Given — a session with no recipe, holding one attached document
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());
        attach_document(session.path(), "spec.md", "# Spec\n");

        // When
        let docs = context_docs_for_session("", session.path());

        // Then — no manifest docs, but the attachment is still surfaced
        assert_eq!(
            kinds_and_basenames(&docs),
            vec![(ContextDocKind::Attachment, "spec.md".to_string())]
        );
    }

    #[test]
    fn context_docs_for_an_unknown_recipe_list_only_the_attachments() {
        // Given — a session whose recipe name does not resolve, holding one attached document
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());
        attach_document(session.path(), "spec.md", "# Spec\n");

        // When
        let docs = context_docs_for_session("not-a-real-recipe", session.path());

        // Then — no manifest docs, but the attachment is still surfaced
        assert_eq!(
            kinds_and_basenames(&docs),
            vec![(ContextDocKind::Attachment, "spec.md".to_string())]
        );
    }

    #[test]
    fn a_present_manifest_context_doc_reports_its_on_disk_size() {
        // Given — a pr-stack session whose artifacts/ holds exploration.md
        let session = tempfile::tempdir().unwrap();
        let artifacts = artifacts_dir_in(session.path());
        fs::write(artifacts.join("exploration.md"), "# Exploration\n").unwrap();

        // When
        let docs = context_docs_for_session("pr-stack", session.path());

        // Then — the present doc reports its byte size
        find_doc_of_kind(&docs, ContextDocKind::Manifest, "exploration")
            .assert_exists(true)
            .assert_size_bytes("# Exploration\n".len() as u64);
    }

    #[test]
    fn an_absent_manifest_context_doc_reports_zero_size_bytes() {
        // Given — a pr-stack session whose artifacts/ does not hold stack-plan.yaml
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());

        // When
        let docs = context_docs_for_session("pr-stack", session.path());

        // Then — the missing doc reports zero size
        find_doc_of_kind(&docs, ContextDocKind::Manifest, "stack_plan")
            .assert_exists(false)
            .assert_size_bytes(0);
    }

    #[test]
    fn reading_an_attachment_basename_through_the_context_doc_reader_is_permission_denied() {
        // Given — a pr-stack session with an attached document
        let session = tempfile::tempdir().unwrap();
        artifacts_dir_in(session.path());
        attach_document(session.path(), "notes.md", "meeting notes\n");

        // When — the manifest-only reader is asked for it
        let result = read_session_context_doc_utf8("pr-stack", session.path(), "notes.md");

        // Then — refused: attachments may be binary, so they are listed but not read here
        assert_permission_denied(result);
    }
}
