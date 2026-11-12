//! PRD acceptance: the pr-stack recipe plans into, and resumes from, the project's stack store —
//! the orchestrator's changeset holds only the stack's id.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-project-stack-store.md.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_core::changeset::{
    read_changeset, start_goal_for_session_continue, write_changeset, Changeset, Stack, StackNode,
};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_pr_stack::stack_migration::migrate_session_stacks;
use tddy_workflow_recipes::pr_stack::PrStackHooks;
use tddy_workflow_recipes::PrStackRecipe;

const A_TWO_PR_PLAN: &str = r#"
version: 1
prs:
  - node_id: n1
    title: Token store
    description: ""
    branch_suggestion: feature/auth/token-store
    parents: []
  - node_id: n2
    title: Login API
    description: ""
    branch_suggestion: feature/auth/login-api
    parents: [n1]
"#;

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

    /// A pr-stack session of `project_id` with `changeset` as its manifest.
    fn with_a_pr_stack_session(
        &self,
        session_id: &str,
        project_id: &str,
        changeset: Changeset,
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
        write_changeset(&dir, &changeset).expect("write changeset.yaml");
        dir
    }
}

fn a_legacy_orchestrator_changeset_stuck_at_init() -> Changeset {
    Changeset {
        recipe: Some("pr-stack".to_string()),
        stack: Some(Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Token store".to_string(),
                ..StackNode::default()
            }],
        }),
        ..Changeset::default()
    }
}

fn the_plan_is_written(session_dir: &Path, plan: &str) {
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.to_path_buf());
    ctx.set_sync("output", plan.to_string());
    let result = TaskResult {
        response: plan.to_string(),
        next_action: NextAction::Continue,
        task_id: "write-stack-plan".to_string(),
        status_message: None,
    };
    PrStackHooks::new(None)
        .after_task("write-stack-plan", &ctx, &result)
        .expect("write-stack-plan after_task hook");
}

#[test]
fn a_migrated_orchestrator_resumes_into_orchestrate() {
    // Given — a pre-consolidation orchestrator sitting at "Init" with a populated stack, migrated
    let host = a_host();
    let session = host.with_a_pr_stack_session(
        "sess-orchestrator",
        "proj-auth",
        a_legacy_orchestrator_changeset_stuck_at_init(),
    );
    migrate_session_stacks(host.sessions_base(), &host.projects_dir());
    let migrated = read_changeset(&session).expect("read migrated changeset");
    assert_eq!(
        (migrated.stack.is_none(), migrated.stack_id.is_some()),
        (true, true),
        "precondition: the sweep moved the stack out of the changeset"
    );

    // When
    let goal = start_goal_for_session_continue(&PrStackRecipe, &migrated);

    // Then — the stack it references still means orchestration is under way
    assert_eq!(goal.as_str(), "orchestrate");
}

#[test]
fn a_planned_stack_is_written_to_the_project_store_and_the_changeset_holds_only_its_id() {
    // Given — a fresh pr-stack session of a project
    let host = a_host();
    let session = host.with_a_pr_stack_session(
        "sess-planner",
        "proj-auth",
        Changeset {
            recipe: Some("pr-stack".to_string()),
            ..Changeset::default()
        },
    );

    // When
    the_plan_is_written(&session, A_TWO_PR_PLAN);

    // Then — the changeset references a project record that carries the planned nodes
    let changeset = read_changeset(&session).expect("read changeset");
    assert_eq!(
        changeset.stack, None,
        "the changeset must not carry the stack"
    );
    let stack_id = changeset
        .stack_id
        .expect("the changeset must reference the planned stack by id");
    let record = host
        .store()
        .read("proj-auth", &stack_id)
        .expect("read record");
    let planned: Vec<(&str, Vec<String>)> = record
        .stack
        .nodes
        .iter()
        .map(|n| (n.node_id.as_str(), n.parents.clone()))
        .collect();
    assert_eq!(
        (record.created_by_session_id.as_str(), planned),
        (
            "sess-planner",
            vec![("n1", vec![]), ("n2", vec!["n1".to_string()])]
        )
    );
}
