//! PRD acceptance: the session list's `stack_plan_json` reads the stack from the project's stack
//! store, so the PR-stack screen lists the same planned PRs before and after the startup migration.
//!
//! Feature: docs/ft/coder/pr-stacking.md § Where a stack lives.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::changeset::{write_changeset, Changeset, Stack, StackNode};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_pr_stack::stack_migration::migrate_session_stacks;
use tddy_session_activity::session_list_enrichment::session_list_status_from_session_dir;

fn a_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        branch_suggestion: Some(format!("feature/auth/{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        display_order: Some(node_id[1..].parse().expect("numbered node id")),
        ..StackNode::default()
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

    /// A pr-stack orchestrator written before stacks moved to the project.
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
}

fn the_listed_stack_plan_json(session_dir: &Path) -> String {
    session_list_status_from_session_dir(Some(CodingAgentRegistry::builtin_cached()), session_dir)
        .expect("enrich session row")
        .stack_plan_json
}

fn a_legacy_stack() -> Stack {
    Stack {
        version: 1,
        nodes: vec![
            a_node("n1", &[]),
            a_node("n2", &["n1"]),
            a_node("n3", &["n1"]),
        ],
    }
}

#[test]
fn an_unmigrated_session_lists_no_stack_plan() {
    // Given — a legacy orchestrator the startup sweep has not reached: only the sweep reads the
    // legacy field
    let host = a_host();
    let session =
        host.with_a_legacy_orchestrator("sess-orchestrator", "proj-auth", a_legacy_stack());

    // When
    let listed = the_listed_stack_plan_json(&session);

    // Then
    assert_eq!(listed, "");
}

#[test]
fn a_migrated_sessions_list_reports_the_stack_plan_it_carried_before_migration() {
    // Given — a legacy orchestrator, and the JSON its own stack serializes to
    let host = a_host();
    let legacy_stack = a_legacy_stack();
    let expected = serde_json::to_string(&legacy_stack).expect("serialize the legacy stack");
    let session = host.with_a_legacy_orchestrator("sess-orchestrator", "proj-auth", legacy_stack);

    // When
    migrate_session_stacks(host.sessions_base(), &host.projects_dir());

    // Then
    assert_eq!(the_listed_stack_plan_json(&session), expected);
}
