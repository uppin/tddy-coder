//! PRD acceptance: the startup sweep moves every session-held `Changeset.stack` into its project's
//! stack store and leaves only the record's id behind — once, idempotently, and safely across an
//! interrupted run.
//!
//! Feature: docs/ft/coder/pr-stacking.md § Where a stack lives.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_core::changeset::{read_changeset, write_changeset, Changeset, Stack, StackNode};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_pr_stack::stack_migration::{migrate_session_stacks, MigrationReport};

fn a_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        branch_suggestion: Some(format!("feature/auth/{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..StackNode::default()
    }
}

fn a_two_node_stack() -> Stack {
    Stack {
        version: 1,
        nodes: vec![a_node("n1", &[]), a_node("n2", &["n1"])],
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

    fn projects_dir(&self) -> PathBuf {
        self.root.path().join("projects")
    }

    fn store(&self) -> ProjectStackStore {
        ProjectStackStore::new(self.projects_dir())
    }

    /// A pr-stack orchestrator session written by a daemon before stacks moved to the project: its
    /// changeset carries the whole `stack`. An empty `project_id` is a session that records none.
    fn with_a_legacy_orchestrator(
        &self,
        session_id: &str,
        project_id: &str,
        stack: Stack,
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
            stack: Some(stack),
            ..Changeset::default()
        };
        write_changeset(&dir, &changeset).expect("write changeset.yaml");
        dir
    }

    fn sweep(&self) -> MigrationReport {
        migrate_session_stacks(self.sessions_base(), &self.projects_dir())
    }
}

fn changeset_yaml(session_dir: &Path) -> String {
    std::fs::read_to_string(session_dir.join("changeset.yaml")).expect("read changeset.yaml")
}

#[test]
fn a_legacy_changeset_stack_is_moved_into_the_project_and_replaced_by_its_id() {
    // Given
    let host = a_host();
    let session = host.with_a_legacy_orchestrator("sess-planner", "proj-auth", a_two_node_stack());

    // When
    let report = host.sweep();

    // Then — the changeset holds only the id, and the id names a record carrying the same stack
    assert_eq!(
        report.migrated_session_ids,
        vec!["sess-planner".to_string()]
    );
    let migrated = read_changeset(&session).expect("read migrated changeset");
    assert_eq!(
        migrated.stack, None,
        "the changeset must no longer carry the stack"
    );
    let stack_id = migrated
        .stack_id
        .expect("the changeset must reference its migrated stack by id");
    let record = host
        .store()
        .read("proj-auth", &stack_id)
        .expect("read record");
    assert_eq!(
        (record.created_by_session_id.as_str(), record.stack),
        ("sess-planner", a_two_node_stack())
    );
}

#[test]
fn a_second_sweep_changes_nothing() {
    // Given — a host already swept once
    let host = a_host();
    let session = host.with_a_legacy_orchestrator("sess-planner", "proj-auth", a_two_node_stack());
    host.sweep();
    let changeset_after_first = changeset_yaml(&session);
    let records_after_first = host
        .store()
        .list("proj-auth")
        .expect("list after first sweep");

    // When
    let report = host.sweep();

    // Then
    assert_eq!(report, MigrationReport::default());
    assert_eq!(changeset_yaml(&session), changeset_after_first);
    assert_eq!(
        host.store()
            .list("proj-auth")
            .expect("list after second sweep"),
        records_after_first
    );
}

#[test]
fn a_sweep_interrupted_after_writing_the_stack_completes_on_rerun_without_a_duplicate() {
    // Given — a sweep that died after creating the record but before rewriting the changeset
    let host = a_host();
    let session = host.with_a_legacy_orchestrator("sess-planner", "proj-auth", a_two_node_stack());
    let written_before_the_crash = host
        .store()
        .create("proj-auth", "sess-planner", a_two_node_stack())
        .expect("record written by the interrupted sweep");

    // When
    let report = host.sweep();

    // Then — the existing record is adopted; no second one appears
    assert_eq!(
        report.migrated_session_ids,
        vec!["sess-planner".to_string()]
    );
    let stored: Vec<String> = host
        .store()
        .list("proj-auth")
        .expect("list")
        .into_iter()
        .map(|r| r.stack_id)
        .collect();
    assert_eq!(stored, vec![written_before_the_crash.stack_id.clone()]);
    let migrated = read_changeset(&session).expect("read migrated changeset");
    assert_eq!(
        (migrated.stack_id, migrated.stack),
        (Some(written_before_the_crash.stack_id), None)
    );
}

#[test]
fn a_session_without_a_project_is_left_alone_and_reported() {
    // Given — a legacy orchestrator whose metadata records no project
    let host = a_host();
    let session = host.with_a_legacy_orchestrator("sess-orphan", "", a_two_node_stack());
    let before = changeset_yaml(&session);

    // When
    let report = host.sweep();

    // Then — reported, untouched, and no project directory invented for it
    assert_eq!(
        report,
        MigrationReport {
            sessions_without_project: vec!["sess-orphan".to_string()],
            ..MigrationReport::default()
        }
    );
    assert_eq!(changeset_yaml(&session), before);
    assert!(
        !host.projects_dir().exists(),
        "no projects dir may be created for a session without a project"
    );
}
