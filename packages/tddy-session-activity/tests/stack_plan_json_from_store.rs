//! PRD acceptance: the session list's `work_queue_json` reads the queue the session owns, and is empty
//! for a session that owns none (`#workqueue` 5/14).
//!
//! Feature: docs/ft/coder/pr-stacking.md § Where a stack lives.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_changeset::work_queue_fixtures::seed_session_stack;
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::changeset::{read_session_stack, write_changeset, Changeset, WorkItem, WorkQueue};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_session_activity::session_list_enrichment::session_list_status_from_session_dir;

fn a_node(node_id: &str, parents: &[&str]) -> WorkItem {
    WorkItem {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        branch_suggestion: Some(format!("feature/auth/{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        display_order: Some(node_id[1..].parse().expect("numbered node id")),
        ..WorkItem::default()
    }
}

/// One host's data directory: `sessions/` beside `projects/`, as the daemon lays them out.
struct AHost {
    root: tempfile::TempDir,
}

fn a_host() -> AHost {
    AHost {
        root: tempfile::tempdir().expect("temp sessions base"),
    }
}

impl AHost {
    fn sessions_base(&self) -> &Path {
        self.root.path()
    }

    /// A session; it owns `stack` as its queue when `owns_queue`.
    fn with_a_session(
        &self,
        session_id: &str,
        project_id: &str,
        stack: WorkQueue,
        owns_queue: bool,
    ) -> PathBuf {
        let dir = unified_session_dir_path(self.sessions_base(), session_id);
        std::fs::create_dir_all(&dir).expect("session dir");
        write_initial_tool_session_metadata(
            &dir,
            InitialToolSessionMetadataOpts {
                project_id: project_id.to_string(),
                recipe: Some("pr-stack".to_string()),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write .session.yaml");
        let changeset = Changeset {
            recipe: Some("pr-stack".to_string()),
            ..Changeset::default()
        };
        write_changeset(&dir, &changeset).expect("write changeset.yaml");
        if owns_queue {
            seed_session_stack(&dir, stack);
        }
        dir
    }
}

fn the_listed_stack_plan_json(session_dir: &Path) -> String {
    session_list_status_from_session_dir(Some(CodingAgentRegistry::builtin_cached()), session_dir)
        .expect("enrich session row")
        .work_queue_json
}

fn a_legacy_stack() -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![
            a_node("n1", &[]),
            a_node("n2", &["n1"]),
            a_node("n3", &["n1"]),
        ],
        ..Default::default()
    }
}

#[test]
fn a_session_owning_no_queue_lists_no_stack_plan() {
    // Given
    let host = a_host();
    let session = host.with_a_session("sess-plain", "proj-auth", a_legacy_stack(), false);

    // When
    let listed = the_listed_stack_plan_json(&session);

    // Then
    assert_eq!(listed, "");
}

#[test]
fn a_session_owning_a_queue_lists_it() {
    // Given
    let host = a_host();
    let session = host.with_a_session("sess-owner", "proj-auth", a_legacy_stack(), true);
    // The queue as the session's record holds it — numbered by the store (`WorkQueue.revision`).
    let stored = read_session_stack(&session)
        .expect("read the queue")
        .expect("the session owns a queue");

    // When / Then
    assert_eq!(
        the_listed_stack_plan_json(&session),
        serde_json::to_string(&stored).expect("serialize the queue")
    );
}
