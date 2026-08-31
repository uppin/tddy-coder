//! Acceptance tests: unified worktree base resolution across agent backends.
//!
//! These tests verify that a session spawned with a `stack_parent` pointing at a
//! pr-stack orchestrator bases its worktree off the planned node's parent branch
//! (via `Stack::base_ref_for_spawn`), not `origin/master`. They cover both
//! cursor-cli and claude-cli session types — the cursor-cli path was the
//! regression (hardcoded `None` for the chain base).

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use tddy_core::changeset::{GithubPrStatus, Stack, StackNode};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{write_changeset, Changeset};
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";
const ORCHESTRATOR_SESSION_ID: &str = "019f9dd5-716d-7071-96ac-464ff7b98c2a";

fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn write_config_with_binary(
    stub_binary: &str,
    cursor_cli: bool,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let section = if cursor_cli {
        format!("cursor_cli:\n  binary_path: {stub_binary}\n")
    } else {
        format!("claude_cli:\n  binary_path: {stub_binary}\n")
    };
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
{section}"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git command failed");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().unwrap()]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// The chat id the stub `cursor-agent` mints for `create-chat`.
const STUB_CHAT_ID: &str = "f8db82db-e154-41d0-ae72-312bdf6d4d80";

/// A stub agent that echoes its argv, and mints [`STUB_CHAT_ID`] when asked for a chat.
fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_agent.sh");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then echo \"{STUB_CHAT_ID}\"; exit 0; fi\necho \"ARGV: $@\"\ncat\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

/// Push a branch to origin so `git fetch origin <branch>` succeeds during worktree setup.
fn push_branch_to_origin(repo: &std::path::Path, branch: &str) {
    Command::new("git")
        .args(["checkout", "-b", branch, "origin/main"])
        .current_dir(repo)
        .output()
        .expect("create branch");
    Command::new("git")
        .args(["commit", "--allow-empty", "-m", &format!("on {branch}")])
        .current_dir(repo)
        .output()
        .expect("commit on branch");
    Command::new("git")
        .args(["push", "-u", "origin", branch])
        .current_dir(repo)
        .output()
        .expect("push branch to origin");
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo)
        .output()
        .expect("checkout main");
}

/// A planned stack node with a single parent and a materialized branch.
fn a_materialized_node(node_id: &str, branch: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(branch.to_string()),
        branch: Some(branch.to_string()),
        session_id: None,
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// A planned (unmaterialized) child node that the spawn is about to create.
fn a_planned_child_node(node_id: &str, branch_suggestion: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(branch_suggestion.to_string()),
        branch: None,
        session_id: None,
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// Write a pr-stack orchestrator parent session under `sessions_base` with a
/// two-node stack: a materialized `bottom` node (branch pushed to origin) and a
/// planned `child` node whose `branch_suggestion` is `child_branch`.
fn write_orchestrator_with_materialized_parent(
    sessions_base: &std::path::Path,
    repo: &std::path::Path,
    parent_branch: &str,
    child_branch: &str,
) {
    push_branch_to_origin(repo, parent_branch);

    let orchestrator_dir = unified_session_dir_path(sessions_base, ORCHESTRATOR_SESSION_ID);
    std::fs::create_dir_all(&orchestrator_dir).expect("create orchestrator session dir");

    let stack = Stack {
        version: 1,
        nodes: vec![
            a_materialized_node("bottom", parent_branch, &[]),
            a_planned_child_node("child", child_branch, &["bottom"]),
        ],
    };

    let cs = Changeset {
        recipe: Some("pr-stack".to_string()),
        stack: Some(stack),
        ..Changeset::default()
    };
    write_changeset(&orchestrator_dir, &cs).expect("write orchestrator changeset");
}

fn start_session_request(
    session_type: &str,
    stack_parent: &str,
    new_branch_name: &str,
) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: session_type.to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: new_branch_name.to_string(),
        selected_integration_base_ref: String::new(),
        selected_branch_to_work_on: String::new(),
        initial_prompt: String::new(),
        permission_mode: String::new(),
        stack_parent: stack_parent.to_string(),
        sandbox: false,
        managed_codebase: false,
        specialized_agents: vec![],
        ..Default::default()
    }
}

/// Read `effective_worktree_integration_base_ref` from the spawned session's changeset.
fn effective_base_for_session(sessions_base: &std::path::Path, session_id: &str) -> String {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must exist");
    cs.effective_worktree_integration_base_ref
        .expect("effective_worktree_integration_base_ref must be set")
}

/// **cursor_cli_pr_stack_child_bases_off_planned_node_parent** — A cursor-cli
/// session spawned with a pr-stack orchestrator parent and a `new_branch_name`
/// matching a planned node must base its worktree off that node's nearest
/// non-merged ancestor's `origin/<branch>`, not `origin/master`.
#[tokio::test]
async fn cursor_cli_pr_stack_child_bases_off_planned_node_parent() {
    // Given — a repo with origin, a materialized parent branch, and an orchestrator
    // session whose stack has a planned child node.
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let parent_branch = "feature/stack/parent";
    let child_branch = "feature/stack/child";
    write_orchestrator_with_materialized_parent(
        sessions_tmp.path(),
        repo_dir.path(),
        parent_branch,
        child_branch,
    );

    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap(), true);
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When — start a cursor-cli session chaining onto the orchestrator.
    let resp = service
        .start_session(Request::new(start_session_request(
            "cursor-cli",
            ORCHESTRATOR_SESSION_ID,
            child_branch,
        )))
        .await
        .expect("StartSession cursor-cli with stack_parent must succeed");

    // Then — the worktree bases off the parent node's branch, not origin/master.
    let session_id = resp.into_inner().session_id;
    let effective_base = effective_base_for_session(sessions_tmp.path(), &session_id);
    assert_eq!(
        effective_base,
        format!("origin/{parent_branch}"),
        "cursor-cli pr-stack child must base off the planned node's parent branch, not origin/master"
    );
}

/// **claude_cli_pr_stack_child_bases_off_planned_node_parent** — Same behavior
/// for claude-cli (regression: the claude-cli path already resolves correctly).
#[tokio::test]
async fn claude_cli_pr_stack_child_bases_off_planned_node_parent() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let parent_branch = "feature/stack/parent";
    let child_branch = "feature/stack/child";
    write_orchestrator_with_materialized_parent(
        sessions_tmp.path(),
        repo_dir.path(),
        parent_branch,
        child_branch,
    );

    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap(), false);
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .start_session(Request::new(start_session_request(
            "claude-cli",
            ORCHESTRATOR_SESSION_ID,
            child_branch,
        )))
        .await
        .expect("StartSession claude-cli with stack_parent must succeed");

    // Then
    let session_id = resp.into_inner().session_id;
    let effective_base = effective_base_for_session(sessions_tmp.path(), &session_id);
    assert_eq!(
        effective_base,
        format!("origin/{parent_branch}"),
        "claude-cli pr-stack child must base off the planned node's parent branch"
    );
}

/// **cursor_cli_session_without_stack_parent_uses_default_base** — Without a
/// stack_parent, the session bases off the default integration base (origin/main),
/// not an error. Regression guard.
#[tokio::test]
async fn cursor_cli_session_without_stack_parent_uses_default_base() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap(), true);
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When — no stack_parent.
    let resp = service
        .start_session(Request::new(start_session_request(
            "cursor-cli",
            "",
            "feature/no-stack/child",
        )))
        .await
        .expect("StartSession cursor-cli without stack_parent must succeed");

    // Then — bases off origin/main (the default for this repo).
    let session_id = resp.into_inner().session_id;
    let effective_base = effective_base_for_session(sessions_tmp.path(), &session_id);
    assert_eq!(
        effective_base, "origin/main",
        "cursor-cli without stack_parent must base off the default integration base"
    );
}

// ---------------------------------------------------------------------------
// An explicit base-branch override bypasses the stack ordering gate.
//
// The daemon's spawn gate (`Stack::base_ref_for_spawn`) refuses a child whose
// non-merged parent owns no branch. That is correct for the *default* path: the
// resolver would otherwise silently cut the child from `origin/<default>` for a
// parent that has not pushed yet. But the Start-session dialog also lets the
// operator pick a different base branch (`selected_integration_base_ref`),
// and that choice is a deliberate repoint — not a guess. The gate must not run
// for it, or a child of a parent whose PR was merged externally (branch gone,
// `pr_status.phase` still `"open"` because the daemon never merged it) cannot
// be started at all, even though the merged work is already in the chosen base.
// ---------------------------------------------------------------------------

/// A parent node whose PR was merged **externally**: the branch is gone (`branch: None`) and the
/// daemon never updated `pr_status.phase`, so it still reads `"open"`. This is the exact shape of
/// `workspace-tool-sandbox` in the reported incident.
fn an_externally_merged_parent_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(format!("feature/stack/{node_id}")),
        branch: None,
        session_id: Some(format!("session-for-{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: Some(GithubPrStatus {
            phase: "open".to_string(),
            url: Some(format!("https://github.com/acme/repo/pull/{node_id}")),
            error: None,
        }),
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// Write a pr-stack orchestrator whose planned `child` node depends on a parent `bottom` that was
/// merged externally — branchless and still `"open"`. No branch is pushed for `bottom`: the merge
/// deleted it on origin, and the child is meant to base off the project default instead.
fn write_orchestrator_with_externally_merged_parent(
    sessions_base: &std::path::Path,
    child_branch: &str,
) {
    let orchestrator_dir = unified_session_dir_path(sessions_base, ORCHESTRATOR_SESSION_ID);
    std::fs::create_dir_all(&orchestrator_dir).expect("create orchestrator session dir");

    let stack = Stack {
        version: 1,
        nodes: vec![
            an_externally_merged_parent_node("bottom", &[]),
            a_planned_child_node("child", child_branch, &["bottom"]),
        ],
    };

    let cs = Changeset {
        recipe: Some("pr-stack".to_string()),
        stack: Some(stack),
        ..Changeset::default()
    };
    write_changeset(&orchestrator_dir, &cs).expect("write orchestrator changeset");
}

/// **explicit_base_override_bypasses_the_gate_for_an_externally_merged_parent** — When the
/// operator picks a base branch in the Start-session dialog, that choice is sent as
/// `selected_integration_base_ref` and must bypass the stack ordering gate. A child of a parent
/// whose PR was merged externally (branch gone, `pr_status.phase` still `"open"`) then spawns off
/// the chosen base instead of being refused with `failed_precondition`.
#[tokio::test]
async fn explicit_base_override_bypasses_the_gate_for_an_externally_merged_parent() {
    // Given — a repo with origin/main, and an orchestrator whose `child` node depends on a
    // parent `bottom` that was merged externally: branchless, `pr_status.phase` still `"open"`.
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let child_branch = "feature/stack/child";
    write_orchestrator_with_externally_merged_parent(sessions_tmp.path(), child_branch);

    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap(), false);
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When — the operator starts `child` naming the planned node and choosing `origin/main` as the
    // base, the deliberate escape from a stack whose parent has nothing left to offer.
    let resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: child_branch.to_string(),
            selected_integration_base_ref: "origin/main".to_string(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: ORCHESTRATOR_SESSION_ID.to_string(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect("an explicit base override must bypass the ordering gate and spawn the child");

    // Then — the child bases off the operator-chosen `origin/main`, where the externally merged
    // parent's work already lives.
    let session_id = resp.into_inner().session_id;
    let effective_base = effective_base_for_session(sessions_tmp.path(), &session_id);
    assert_eq!(
        effective_base, "origin/main",
        "an explicit base override must base the child off the chosen ref, not refuse the spawn"
    );
}

/// **empty_base_override_still_refuses_when_the_parent_is_branchless_and_non_merged** — The gate
/// still guards the default path: an empty `selected_integration_base_ref` falls through to the
/// stack-parent resolution, which runs `base_ref_for_spawn` and refuses on a branchless non-merged
/// parent. The fix bypasses the gate only for an *explicit* override, never silently.
#[tokio::test]
async fn empty_base_override_still_refuses_when_the_parent_is_branchless_and_non_merged() {
    // Given — the same externally-merged-parent stack as above.
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let child_branch = "feature/stack/child";
    write_orchestrator_with_externally_merged_parent(sessions_tmp.path(), child_branch);

    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_binary(stub.to_str().unwrap(), false);
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When — the operator names the planned node but sends no base override (the default path).
    let err = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: child_branch.to_string(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: ORCHESTRATOR_SESSION_ID.to_string(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect_err(
            "the default path must still be refused when a non-merged parent owns no branch",
        );

    // Then — the ordering gate refuses, naming the branchless parent, exactly as before.
    assert_eq!(
        err.code(),
        tddy_rpc::Code::FailedPrecondition,
        "the default path must fail with failed_precondition, not succeed silently"
    );
    assert!(
        err.message().contains("non-merged parent 'bottom'"),
        "the refusal must name the branchless parent; got: {}",
        err.message()
    );
}
