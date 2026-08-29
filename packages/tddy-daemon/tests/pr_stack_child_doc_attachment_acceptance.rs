//! PRD acceptance ([docs/ft/coder/pr-stack-docs.md] § Attaching documents to the child session): a
//! child spawned for a planned PR receives four documents from the orchestrator — its own PRD and
//! changeset, plus the shared plan and the exploration map.
//!
//! Every one is a `HostDocumentRef` under `SESSION_ARTIFACT`, so the source may be **nested**
//! (`prs/n1/PRD.md`) while the destination stays a **flat basename** — the attachment store is one
//! level deep by design. The ref names the *orchestrator's* host, which is what makes a cross-host
//! stack work without copying anything up front.
//!
//! Two rules are pinned here because they are easy to regress: a document that does not exist yet is
//! **skipped, not fatal** (starting a node before the docs pass has run is sometimes correct), and an
//! attached `PRD.md` never displaces the child recipe's own `artifacts/PRD.md`.

use std::fs;
use std::path::Path;

use tddy_daemon::stack_doc_attachments::stack_doc_attachments;
use tddy_service::proto::connection::{
    session_attachment::Source, HostDocumentRef, HostDocumentScope, SessionAttachment,
};

const ORCHESTRATOR_ID: &str = "018f1111-aaaa-7000-1111-000000000001";
const ORCHESTRATOR_HOST: &str = "daemon-alpha";

// ── Builders ────────────────────────────────────────────────────────────────────────────────

/// An orchestrator session directory. `documented_nodes` get a full `prs/<id>/` pair; the shared
/// plan and exploration map are written unless suppressed.
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

    fn with_shared_documents(self) -> Self {
        fs::write(
            self.artifacts().join("pr-stack-plan.md"),
            "# PR Stack Plan\n",
        )
        .expect("write plan");
        fs::write(self.artifacts().join("exploration.md"), "# Exploration\n")
            .expect("write exploration");
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
}

// ── Assertions ──────────────────────────────────────────────────────────────────────────────

trait AttachmentAssertions {
    fn assert_destinations(&self, expected: &[&str]) -> &Self;
    fn assert_source_of(&self, basename: &str, relative_path: &str) -> &Self;
    fn assert_every_source_targets_session_artifacts_of(&self, session_id: &str) -> &Self;
    fn assert_every_source_reads_host(&self, daemon_instance_id: &str) -> &Self;
}

impl AttachmentAssertions for Vec<SessionAttachment> {
    fn assert_destinations(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|a| a.basename.as_str()).collect();
        assert_eq!(actual, expected, "attached document set mismatch");
        self
    }

    fn assert_source_of(&self, basename: &str, relative_path: &str) -> &Self {
        let attachment = self
            .iter()
            .find(|a| a.basename == basename)
            .unwrap_or_else(|| panic!("no attachment destined for '{basename}'"));
        let host_document = host_document_of(attachment);
        assert_eq!(
            host_document.relative_path, relative_path,
            "'{basename}' must be read from '{relative_path}' on the orchestrator"
        );
        self
    }

    fn assert_every_source_targets_session_artifacts_of(&self, session_id: &str) -> &Self {
        for attachment in self {
            let host_document = host_document_of(attachment);
            assert_eq!(
                host_document.scope,
                HostDocumentScope::SessionArtifact as i32,
                "'{}' must be read under SESSION_ARTIFACT",
                attachment.basename
            );
            assert_eq!(
                host_document.session_id, session_id,
                "'{}' must be read from the orchestrator's session",
                attachment.basename
            );
        }
        self
    }

    fn assert_every_source_reads_host(&self, daemon_instance_id: &str) -> &Self {
        for attachment in self {
            let host_document = host_document_of(attachment);
            assert_eq!(
                host_document.daemon_instance_id, daemon_instance_id,
                "'{}' must be read from the orchestrator's host, not the spawning daemon",
                attachment.basename
            );
        }
        self
    }
}

fn host_document_of(attachment: &SessionAttachment) -> &HostDocumentRef {
    match attachment.source.as_ref() {
        Some(Source::HostDocument(host_document)) => host_document,
        Some(Source::Staged(_)) => panic!(
            "'{}' must be attached by host-document reference, not staged bytes",
            attachment.basename
        ),
        None => panic!("'{}' carries no source", attachment.basename),
    }
}

// ── A fully documented node ──────────────────────────────────────────────────────────────────

#[test]
fn a_documented_node_attaches_its_own_pair_and_the_two_shared_documents() {
    // Given — an orchestrator that has run its docs pass
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then
    attachments.assert_destinations(&[
        "PRD.md",
        "changeset.md",
        "pr-stack-plan.md",
        "exploration.md",
    ]);
}

#[test]
fn a_nodes_own_documents_are_read_from_its_directory() {
    // Given
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n2");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n2");

    // Then — a nested source resolving to a flat destination
    attachments
        .assert_source_of("PRD.md", "prs/n2/PRD.md")
        .assert_source_of("changeset.md", "prs/n2/changeset.md");
}

#[test]
fn the_shared_documents_are_read_from_the_artifacts_root() {
    // Given
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then
    attachments
        .assert_source_of("pr-stack-plan.md", "pr-stack-plan.md")
        .assert_source_of("exploration.md", "exploration.md");
}

#[test]
fn every_document_is_read_from_the_orchestrators_session_artifacts() {
    // Given
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then
    attachments.assert_every_source_targets_session_artifacts_of(ORCHESTRATOR_ID);
}

/// The child may be spawned by a different daemon than the one holding the orchestrator. The ref has
/// to name the orchestrator's host, or the fetch reads an empty artifacts directory on the wrong
/// machine and every document silently disappears.
#[test]
fn every_document_is_read_from_the_host_that_owns_the_orchestrator() {
    // Given
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then
    attachments.assert_every_source_reads_host(ORCHESTRATOR_HOST);
}

#[test]
fn every_destination_is_a_flat_basename() {
    // Given
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then — the attachment store is one flat level; a separator here would be refused downstream
    for attachment in &attachments {
        assert!(
            !attachment.basename.contains('/') && !attachment.basename.contains('\\'),
            "destination '{}' must be a flat basename",
            attachment.basename
        );
    }
}

// ── Missing documents are skipped, never fatal ───────────────────────────────────────────────

#[test]
fn a_node_started_before_the_docs_pass_attaches_only_the_shared_documents() {
    // Given — a stack planned but not yet documented
    let session = an_orchestrator_session().with_shared_documents();

    // When — the operator starts a node early
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then — starting early is allowed; the node simply has less context
    attachments.assert_destinations(&["pr-stack-plan.md", "exploration.md"]);
}

#[test]
fn an_orchestrator_with_no_exploration_map_attaches_the_documents_it_has() {
    // Given — exploration is optional; a blank submit writes no file
    let session = an_orchestrator_session().with_documents_for("n1");
    fs::write(
        session.artifacts().join("pr-stack-plan.md"),
        "# PR Stack Plan\n",
    )
    .expect("write plan");

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then
    attachments.assert_destinations(&["PRD.md", "changeset.md", "pr-stack-plan.md"]);
}

#[test]
fn an_orchestrator_with_nothing_written_attaches_no_documents() {
    // Given
    let session = an_orchestrator_session();

    // When
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n1");

    // Then — an empty list, not a refusal: the spawn still succeeds
    assert_eq!(
        attachments,
        Vec::new(),
        "an undocumented orchestrator must attach nothing rather than fail the spawn"
    );
}

#[test]
fn a_node_without_documents_does_not_borrow_another_nodes() {
    // Given — n1 is documented, n2 is not
    let session = an_orchestrator_session()
        .with_shared_documents()
        .with_documents_for("n1");

    // When — n2 is started
    let attachments =
        stack_doc_attachments(session.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, "n2");

    // Then — attaching n1's boundaries to n2 would be worse than attaching none
    attachments.assert_destinations(&["pr-stack-plan.md", "exploration.md"]);
}
