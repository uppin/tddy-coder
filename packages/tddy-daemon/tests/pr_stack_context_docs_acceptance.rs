//! PRD acceptance ([docs/ft/coder/pr-stack-docs.md] § Listing per-PR documents): an orchestrator's
//! `context_docs` list its per-PR documents alongside its stack-level artifacts, and every row
//! carries the **relative path** a client needs to reference it.
//!
//! `known_artifacts()` returns `&'static` basenames and cannot enumerate N per-node files, so the
//! static manifest is left alone and per-PR rows are discovered by scanning `artifacts/prs/*/` —
//! exactly how attachment rows already join the list. The rows are `MANIFEST`, not `ATTACHMENT`:
//! they are recipe-authored, not user-attached.
//!
//! `relative_path` exists because the client cannot derive it. `HostDocumentPicker` reconstructs a
//! path from `kind` + `basename`, which can express `PRD.md` and `attachments/PRD.md` but never
//! `prs/n1/PRD.md` — and the server has known the path all along.

use std::fs;
use std::path::Path;

use tddy_daemon::session_context_docs::{context_docs_for_session, ContextDoc, ContextDocKind};

// ── Builders ────────────────────────────────────────────────────────────────────────────────

struct OrchestratorSession {
    dir: tempfile::TempDir,
}

fn an_orchestrator_session() -> OrchestratorSession {
    let dir = tempfile::tempdir().expect("temp session dir");
    fs::create_dir_all(dir.path().join("artifacts")).expect("artifacts root");
    OrchestratorSession { dir }
}

impl OrchestratorSession {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn artifacts(&self) -> std::path::PathBuf {
        self.dir.path().join("artifacts")
    }

    fn with_stack_level_artifacts(self) -> Self {
        fs::write(self.artifacts().join("stack-plan.yaml"), "version: 1\n").expect("write plan");
        fs::write(
            self.artifacts().join("pr-stack-plan.md"),
            "# PR Stack Plan\n",
        )
        .expect("write plan md");
        self
    }

    fn with_documents_for(self, node_id: &str) -> Self {
        let node_dir = self.artifacts().join("prs").join(node_id);
        fs::create_dir_all(&node_dir).expect("node docs dir");
        fs::write(node_dir.join("PRD.md"), format!("# {node_id} — PRD\n")).expect("write prd");
        fs::write(
            node_dir.join("changeset.md"),
            format!("# Changeset: {node_id}\n"),
        )
        .expect("write changeset");
        self
    }

    fn with_attachment(self, basename: &str) -> Self {
        let attachments = self.artifacts().join("attachments");
        fs::create_dir_all(&attachments).expect("attachments dir");
        fs::write(attachments.join(basename), "attached\n").expect("write attachment");
        self
    }

    fn with_a_stray_file_under(self, node_id: &str, basename: &str) -> Self {
        let node_dir = self.artifacts().join("prs").join(node_id);
        fs::create_dir_all(&node_dir).expect("node docs dir");
        fs::write(node_dir.join(basename), "stray\n").expect("write stray");
        self
    }

    fn context_docs(&self) -> Vec<ContextDoc> {
        context_docs_for_session("pr-stack", self.path())
    }
}

// ── Assertions ──────────────────────────────────────────────────────────────────────────────

trait ContextDocAssertions {
    fn assert_relative_path_of(&self, basename: &str, expected: &str) -> &Self;
    fn assert_kind_of(&self, relative_path: &str, expected: ContextDocKind) -> &Self;
    fn assert_lists_relative_paths(&self, expected: &[&str]) -> &Self;
    fn assert_omits_relative_path(&self, unexpected: &str) -> &Self;
}

impl ContextDocAssertions for Vec<ContextDoc> {
    fn assert_relative_path_of(&self, basename: &str, expected: &str) -> &Self {
        let doc = self
            .iter()
            .find(|d| d.basename == basename)
            .unwrap_or_else(|| panic!("no context doc named '{basename}'"));
        assert_eq!(
            doc.relative_path, expected,
            "'{basename}' must carry the path a client references it by"
        );
        self
    }

    fn assert_kind_of(&self, relative_path: &str, expected: ContextDocKind) -> &Self {
        let doc = self
            .iter()
            .find(|d| d.relative_path == relative_path)
            .unwrap_or_else(|| panic!("no context doc at '{relative_path}'"));
        assert_eq!(doc.kind, expected, "'{relative_path}' has the wrong kind");
        self
    }

    fn assert_lists_relative_paths(&self, expected: &[&str]) -> &Self {
        let existing: Vec<&str> = self
            .iter()
            .filter(|d| d.exists)
            .map(|d| d.relative_path.as_str())
            .collect();
        assert_eq!(existing, expected, "listed document set mismatch");
        self
    }

    fn assert_omits_relative_path(&self, unexpected: &str) -> &Self {
        assert!(
            !self.iter().any(|d| d.relative_path == unexpected),
            "'{unexpected}' must not be listed as a context document"
        );
        self
    }
}

// ── Per-PR documents join the list ───────────────────────────────────────────────────────────

#[test]
fn each_planned_prs_documents_are_listed() {
    // Given — an orchestrator whose docs pass has covered two nodes
    let session = an_orchestrator_session()
        .with_stack_level_artifacts()
        .with_documents_for("n1")
        .with_documents_for("n2");

    // When
    let docs = session.context_docs();

    // Then — stack-level rows first, then the per-PR pairs in node order
    docs.assert_lists_relative_paths(&[
        "stack-plan.yaml",
        "pr-stack-plan.md",
        "prs/n1/PRD.md",
        "prs/n1/changeset.md",
        "prs/n2/PRD.md",
        "prs/n2/changeset.md",
    ]);
}

#[test]
fn a_per_pr_document_carries_its_nested_relative_path() {
    // Given
    let session = an_orchestrator_session().with_documents_for("n1");

    // When
    let docs = session.context_docs();

    // Then — the path the picker offers and an attachment references
    docs.assert_relative_path_of("changeset.md", "prs/n1/changeset.md");
}

#[test]
fn a_per_pr_document_is_recipe_owned_rather_than_user_attached() {
    // Given
    let session = an_orchestrator_session().with_documents_for("n1");

    // When
    let docs = session.context_docs();

    // Then
    docs.assert_kind_of("prs/n1/PRD.md", ContextDocKind::Manifest);
}

#[test]
fn a_stack_level_artifact_carries_its_own_basename_as_its_relative_path() {
    // Given
    let session = an_orchestrator_session().with_stack_level_artifacts();

    // When
    let docs = session.context_docs();

    // Then
    docs.assert_relative_path_of("pr-stack-plan.md", "pr-stack-plan.md");
}

#[test]
fn an_attachment_carries_its_attachments_prefixed_relative_path() {
    // Given — the derivation the picker used to perform client-side
    let session = an_orchestrator_session().with_attachment("requirements.pdf");

    // When
    let docs = session.context_docs();

    // Then
    docs.assert_relative_path_of("requirements.pdf", "attachments/requirements.pdf");
}

// ── What is not a document ───────────────────────────────────────────────────────────────────

/// A node directory holds exactly two documents. Anything else — an editor backup, a stray note the
/// agent wrote — must not be offered as though the recipe authored it.
#[test]
fn a_stray_file_in_a_node_directory_is_not_listed() {
    // Given
    let session = an_orchestrator_session()
        .with_documents_for("n1")
        .with_a_stray_file_under("n1", "scratch.md");

    // When
    let docs = session.context_docs();

    // Then
    docs.assert_omits_relative_path("prs/n1/scratch.md");
}

#[test]
fn an_orchestrator_without_a_docs_pass_lists_only_its_stack_level_artifacts() {
    // Given
    let session = an_orchestrator_session().with_stack_level_artifacts();

    // When
    let docs = session.context_docs();

    // Then
    docs.assert_lists_relative_paths(&["stack-plan.yaml", "pr-stack-plan.md"]);
}
