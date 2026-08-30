//! The documents a planned PR's child session is spawned with.
//!
//! PRD: `docs/ft/coder/pr-stack-docs.md` § Attaching documents to the child session.
//!
//! One helper feeds both spawn paths — the agent's `pr_spawn_child` and the web Start-session
//! dialog. A child that differs by how it was started is a bug the operator cannot see.
//!
//! Every document is attached **by reference**: a `HostDocumentRef` naming the orchestrator's own
//! session and host, which the materializer fetches (streaming, if that host is not this daemon).
//! Nothing is copied up front, so a cross-host stack works unchanged. The source may be nested
//! (`prs/n1/PRD.md`) because `SESSION_ARTIFACT` resolves a full relative path; the destination is a
//! flat basename because the attachment store is one level deep.

use std::path::Path;

use tddy_service::proto::connection::{
    session_attachment::Source, HostDocumentRef, HostDocumentScope, SessionAttachment,
};
use tddy_workflow::{session_artifacts_root, SESSION_ATTACHMENTS_SUBDIR};
use tddy_workflow_recipes::plan_pr_stack::PR_STACK_PLAN_MD_BASENAME;
use tddy_workflow_recipes::pr_stack::{
    NODE_CHANGESET_BASENAME, NODE_DOCS_SUBDIR, NODE_PRD_BASENAME,
};
use tddy_workflow_recipes::writer::EXPLORATION_BASENAME;

/// The four documents a child is offered, in listing order: the node's own pair, then the shared
/// stack-level pair.
///
/// A document that does not exist is **skipped, not fatal** — starting a node before the docs pass
/// has run is sometimes correct, and failing the spawn would make the docs pass a hard prerequisite
/// for all work. An orchestrator with nothing written yields an empty list.
pub fn stack_doc_attachments(
    orchestrator_session_dir: &Path,
    orchestrator_session_id: &str,
    orchestrator_daemon_instance_id: &str,
    node_id: &str,
) -> Vec<SessionAttachment> {
    let artifacts_root = session_artifacts_root(orchestrator_session_dir);
    let node_docs_dir = format!("{NODE_DOCS_SUBDIR}/{node_id}");
    let offered = [
        format!("{node_docs_dir}/{NODE_PRD_BASENAME}"),
        format!("{node_docs_dir}/{NODE_CHANGESET_BASENAME}"),
        PR_STACK_PLAN_MD_BASENAME.to_string(),
        EXPLORATION_BASENAME.to_string(),
    ];

    offered
        .into_iter()
        .filter(|relative_path| artifacts_root.join(relative_path).is_file())
        .map(|relative_path| SessionAttachment {
            // The destination is the source's last segment: the attachment store is one flat
            // level, so `prs/n1/PRD.md` lands as `PRD.md`.
            basename: relative_path
                .rsplit('/')
                .next()
                .unwrap_or(&relative_path)
                .to_string(),
            source: Some(Source::HostDocument(HostDocumentRef {
                daemon_instance_id: orchestrator_daemon_instance_id.to_string(),
                scope: HostDocumentScope::SessionArtifact as i32,
                session_id: orchestrator_session_id.to_string(),
                project_id: String::new(),
                relative_path,
            })),
        })
        .collect()
}

/// Whether an attachment is the per-PR changeset a `write-stack-docs` pass authored — as opposed to
/// a file the operator happened to attach under the same name.
///
/// Recognised by its **source**, not its destination: only a `SESSION_ARTIFACT` document read from
/// `prs/<node_id>/changeset.md` on the orchestrator qualifies. Matching the destination basename
/// instead would fire for anyone who drags a file called `changeset.md` into the Start-session
/// dialog, and would miss the same document once the operator renames the row.
fn is_node_changeset(attachment: &SessionAttachment) -> bool {
    let Some(Source::HostDocument(host_document)) = attachment.source.as_ref() else {
        return false;
    };
    if host_document.scope != HostDocumentScope::SessionArtifact as i32 {
        return false;
    }
    let segments: Vec<&str> = host_document.relative_path.split('/').collect();
    matches!(
        segments.as_slice(),
        [NODE_DOCS_SUBDIR, node_id, NODE_CHANGESET_BASENAME] if !node_id.is_empty()
    )
}

/// The line appended to a child's `initial_prompt` naming its attached changeset, so the agent reads
/// its boundaries before writing code rather than finding the file by chance.
///
/// Mirrors the grill-me hand-off, which names the brief's path in the spawned conversation's prompt.
/// Returns `None` when no changeset was attached — there is nothing to point at.
pub fn attached_changeset_prompt_line(attachments: &[SessionAttachment]) -> Option<String> {
    attachments
        .iter()
        .find(|a| is_node_changeset(a))
        .map(|attachment| {
            format!(
            "Read your changeset at artifacts/{SESSION_ATTACHMENTS_SUBDIR}/{} before writing code \
             — it states this PR's responsibility, its boundaries, and what each dependency \
             delivers.",
            attachment.basename
        )
        })
}

/// A child's `initial_prompt`, extended with [`attached_changeset_prompt_line`] when its documents
/// include the node's changeset.
///
/// The single rule both spawn paths apply — the agent's `pr_spawn_child` and the operator's
/// Start-session dialog — so a child cannot differ by how it was started. Fed the attachments that
/// **actually materialized**, never the ones that were asked for: a prompt naming a document the
/// child does not hold sends the agent hunting for a file that is not there.
pub fn prompt_with_attached_changeset(
    initial_prompt: &str,
    materialized: &[SessionAttachment],
) -> String {
    match attached_changeset_prompt_line(materialized) {
        Some(line) => format!("{initial_prompt}\n\n{line}"),
        None => initial_prompt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tddy_service::proto::connection::{
        session_attachment::Source, HostDocumentRef, HostDocumentScope, StagedAttachmentRef,
    };

    const ORCHESTRATOR_ID: &str = "018f1111-aaaa-7000-1111-000000000001";
    const ORCHESTRATOR_HOST: &str = "daemon-alpha";

    // ── Builders ─────────────────────────────────────────────────────────────────────────────

    struct Orchestrator {
        dir: tempfile::TempDir,
    }

    fn an_orchestrator() -> Orchestrator {
        let dir = tempfile::tempdir().expect("temp session dir");
        fs::create_dir_all(dir.path().join("artifacts")).expect("artifacts root");
        Orchestrator { dir }
    }

    impl Orchestrator {
        fn artifacts(&self) -> std::path::PathBuf {
            self.dir.path().join("artifacts")
        }

        fn with_shared_documents(self) -> Self {
            fs::write(self.artifacts().join("pr-stack-plan.md"), "# Plan\n").expect("plan");
            fs::write(self.artifacts().join("exploration.md"), "# Exploration\n")
                .expect("exploration");
            self
        }

        fn with_documents_for(self, node_id: &str) -> Self {
            let node_dir = self.artifacts().join("prs").join(node_id);
            fs::create_dir_all(&node_dir).expect("node dir");
            fs::write(node_dir.join("PRD.md"), "# PRD\n").expect("prd");
            fs::write(node_dir.join("changeset.md"), "# Changeset\n").expect("changeset");
            self
        }

        fn attachments_for(&self, node_id: &str) -> Vec<SessionAttachment> {
            stack_doc_attachments(self.dir.path(), ORCHESTRATOR_ID, ORCHESTRATOR_HOST, node_id)
        }
    }

    // ── Assertions ───────────────────────────────────────────────────────────────────────────

    fn destinations(attachments: &[SessionAttachment]) -> Vec<&str> {
        attachments.iter().map(|a| a.basename.as_str()).collect()
    }

    fn source_of<'a>(attachments: &'a [SessionAttachment], basename: &str) -> &'a HostDocumentRef {
        let attachment = attachments
            .iter()
            .find(|a| a.basename == basename)
            .unwrap_or_else(|| panic!("no attachment destined for '{basename}'"));
        match attachment.source.as_ref() {
            Some(Source::HostDocument(host_document)) => host_document,
            _ => panic!("'{basename}' must be attached by host-document reference"),
        }
    }

    // ── The list ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_documented_node_is_offered_its_own_pair_then_the_shared_pair() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        assert_eq!(
            destinations(&attachments),
            vec![
                "PRD.md",
                "changeset.md",
                "pr-stack-plan.md",
                "exploration.md"
            ]
        );
    }

    #[test]
    fn a_nodes_own_documents_are_sourced_from_its_directory() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n2");

        // When
        let attachments = orchestrator.attachments_for("n2");

        // Then — nested source, flat destination
        assert_eq!(
            source_of(&attachments, "PRD.md").relative_path,
            "prs/n2/PRD.md"
        );
        assert_eq!(
            source_of(&attachments, "changeset.md").relative_path,
            "prs/n2/changeset.md"
        );
    }

    #[test]
    fn the_shared_documents_are_sourced_from_the_artifacts_root() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        assert_eq!(
            source_of(&attachments, "pr-stack-plan.md").relative_path,
            "pr-stack-plan.md"
        );
    }

    #[test]
    fn every_document_is_read_under_the_session_artifact_scope() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        for attachment in &attachments {
            let source = source_of(&attachments, &attachment.basename);
            assert_eq!(source.scope, HostDocumentScope::SessionArtifact as i32);
            assert_eq!(source.session_id, ORCHESTRATOR_ID);
        }
    }

    /// The spawning daemon may not be the one holding the orchestrator. Name the wrong host and the
    /// fetch reads an empty artifacts directory on the wrong machine — every document silently gone.
    #[test]
    fn every_document_is_read_from_the_host_owning_the_orchestrator() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        for attachment in &attachments {
            assert_eq!(
                source_of(&attachments, &attachment.basename).daemon_instance_id,
                ORCHESTRATOR_HOST
            );
        }
    }

    #[test]
    fn every_destination_is_a_flat_basename() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then — a separator here is refused by the attachment store downstream
        for attachment in &attachments {
            assert!(
                !attachment.basename.contains('/'),
                "destination '{}' must be flat",
                attachment.basename
            );
        }
    }

    // ── Missing documents ────────────────────────────────────────────────────────────────────

    #[test]
    fn a_node_without_documents_is_offered_only_the_shared_pair() {
        // Given — the plan exists, the docs pass has not run
        let orchestrator = an_orchestrator().with_shared_documents();

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        assert_eq!(
            destinations(&attachments),
            vec!["pr-stack-plan.md", "exploration.md"]
        );
    }

    #[test]
    fn a_node_without_documents_is_never_offered_another_nodes() {
        // Given — n1 is documented, n2 is not
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");

        // When
        let attachments = orchestrator.attachments_for("n2");

        // Then — attaching n1's boundaries to n2 is worse than attaching none
        assert_eq!(
            destinations(&attachments),
            vec!["pr-stack-plan.md", "exploration.md"]
        );
    }

    #[test]
    fn an_orchestrator_with_nothing_written_offers_no_documents() {
        // Given
        let orchestrator = an_orchestrator();

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then — an empty list, not a refusal: the spawn still succeeds
        assert_eq!(attachments, Vec::new());
    }

    #[test]
    fn an_orchestrator_without_an_exploration_map_offers_the_rest() {
        // Given — exploration is optional; a blank submit writes no file
        let orchestrator = an_orchestrator().with_documents_for("n1");
        fs::write(
            orchestrator.artifacts().join("pr-stack-plan.md"),
            "# Plan\n",
        )
        .expect("plan");

        // When
        let attachments = orchestrator.attachments_for("n1");

        // Then
        assert_eq!(
            destinations(&attachments),
            vec!["PRD.md", "changeset.md", "pr-stack-plan.md"]
        );
    }

    // ── The prompt line ──────────────────────────────────────────────────────────────────────

    #[test]
    fn the_prompt_line_names_the_attached_changeset_by_path() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");
        let attachments = orchestrator.attachments_for("n1");

        // When
        let line = attached_changeset_prompt_line(&attachments);

        // Then — the child reads its boundaries before writing code
        assert_eq!(
            line,
            Some("Read your changeset at artifacts/attachments/changeset.md before writing code — it states this PR's responsibility, its boundaries, and what each dependency delivers.".to_string())
        );
    }

    #[test]
    fn no_prompt_line_is_produced_when_no_changeset_was_attached() {
        // Given — a node started before the docs pass ran
        let orchestrator = an_orchestrator().with_shared_documents();
        let attachments = orchestrator.attachments_for("n1");

        // When
        let line = attached_changeset_prompt_line(&attachments);

        // Then — pointing at a file that is not there would send the agent hunting
        assert_eq!(line, None);
    }

    // ── Whose changeset is it ────────────────────────────────────────────────────────────────

    fn an_attached_host_document(basename: &str, relative_path: &str) -> SessionAttachment {
        SessionAttachment {
            basename: basename.to_string(),
            source: Some(Source::HostDocument(HostDocumentRef {
                daemon_instance_id: ORCHESTRATOR_HOST.to_string(),
                scope: HostDocumentScope::SessionArtifact as i32,
                session_id: ORCHESTRATOR_ID.to_string(),
                project_id: String::new(),
                relative_path: relative_path.to_string(),
            })),
        }
    }

    /// The rule runs on every session start, not just a planned PR's. An operator dropping their own
    /// notes into the dialog must not be told to read boundaries nobody wrote.
    #[test]
    fn a_file_the_operator_uploaded_is_not_taken_for_the_nodes_changeset() {
        // Given — staged bytes the operator happened to name `changeset.md`
        let attachments = vec![SessionAttachment {
            basename: NODE_CHANGESET_BASENAME.to_string(),
            source: Some(Source::Staged(StagedAttachmentRef {
                daemon_instance_id: ORCHESTRATOR_HOST.to_string(),
                staging_id: "018f2222-bbbb-7000-2222-000000000002".to_string(),
                file_name: NODE_CHANGESET_BASENAME.to_string(),
            })),
        }];

        // When
        let line = attached_changeset_prompt_line(&attachments);

        // Then
        assert_eq!(line, None);
    }

    #[test]
    fn a_changeset_outside_a_nodes_documents_directory_is_not_the_nodes() {
        // Given — a session artifact of the same name, but not one the docs pass wrote
        let attachments = vec![an_attached_host_document(
            NODE_CHANGESET_BASENAME,
            NODE_CHANGESET_BASENAME,
        )];

        // When
        let line = attached_changeset_prompt_line(&attachments);

        // Then
        assert_eq!(line, None);
    }

    #[test]
    fn a_nodes_changeset_is_recognised_by_where_it_is_read_from() {
        // Given — the dialog lets the operator rename a row, so the destination is not the tell
        let attachments = vec![an_attached_host_document(
            "n1-boundaries.md",
            "prs/n1/changeset.md",
        )];

        // When
        let line = attached_changeset_prompt_line(&attachments);

        // Then — recognised by its source, named by where it actually landed
        assert!(
            line.expect("a node's changeset must be recognised however it was renamed")
                .contains("artifacts/attachments/n1-boundaries.md"),
            "the line must name the destination the document was written to"
        );
    }

    // ── The prompt ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_childs_prompt_gains_the_line_naming_its_changeset() {
        // Given
        let orchestrator = an_orchestrator()
            .with_shared_documents()
            .with_documents_for("n1");
        let attachments = orchestrator.attachments_for("n1");

        // When
        let prompt =
            prompt_with_attached_changeset("Token store\n\nAdds a token store.", &attachments);

        // Then — the node's own brief first, then where to read its boundaries
        assert_eq!(
            prompt,
            format!(
                "Token store\n\nAdds a token store.\n\n{}",
                attached_changeset_prompt_line(&attachments).expect("a changeset was attached")
            )
        );
    }

    #[test]
    fn a_childs_prompt_is_untouched_when_no_changeset_was_attached() {
        // Given — the docs pass has not run for this node
        let orchestrator = an_orchestrator().with_shared_documents();
        let attachments = orchestrator.attachments_for("n1");

        // When
        let prompt = prompt_with_attached_changeset("Token store", &attachments);

        // Then
        assert_eq!(prompt, "Token store");
    }
}
