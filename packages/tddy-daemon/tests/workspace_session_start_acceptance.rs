//! Acceptance tests: starting a `workspace` session — the codebase half of a split session.
//!
//! PRD: docs/ft/daemon/remote-managed-worktree.md
//!
//! Three contracts live here, all of them about a workspace session being asked for by *another*
//! daemon rather than by a human at a form:
//!
//! 1. **The caller may choose the session id.** The daemon placing a split session's worktree on
//!    this host has to be able to name that session before it exists, so that a forwarded
//!    `StartSession` which never answers can still be torn down (§ Failure is atomic).
//! 2. **A branch intent means what it says.** The same request that is refused for a co-located
//!    claude-cli session must be refused here, rather than quietly producing a differently named
//!    branch than the one the operator picked.
//! 3. **The project's configured default branch is honoured**, exactly as the claude-cli path
//!    honours it, so a split session's worktree is not cut from a different base than a co-located
//!    one would have been.
//! 4. **The session records which agent works in the worktree.** A `workspace` session is also what
//!    a standalone checkout and an agent clone's mirror are, and only the half of a split placement
//!    has an agent whose tools a roster could take away — so the pairing is persisted rather than
//!    inferred from the session type (§ Enforced at two layers in
//!    docs/ft/daemon/session-agent-roster.md).

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionAgentsRequest, SplitAgentPlacement,
    StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a38";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A git repository with `main` plus a `release` branch carrying a second commit, and an `origin`
/// remote pointing at itself so the worktree setup's fetch succeeds without a real server.
fn a_git_repo_with_two_branches() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(
        path,
        &["config", "user.email", "acceptance@example.invalid"],
    );
    run_git(path, &["config", "user.name", "Acceptance"]);
    std::fs::write(path.join("README.md"), "acceptance\n").expect("seed file");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-qm", "seed"]);
    run_git(path, &["checkout", "-q", "-b", "release"]);
    std::fs::write(path.join("RELEASE.md"), "release only\n").expect("release file");
    run_git(path, &["add", "RELEASE.md"]);
    run_git(path, &["commit", "-qm", "release"]);
    run_git(path, &["checkout", "-q", "main"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} must succeed in {cwd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn register_project(sessions_base: &Path, repo_path: &Path, main_branch_ref: Option<&str>) {
    let projects_dir = sessions_base.join("projects");
    tddy_daemon::project_storage::write_projects(
        &projects_dir,
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "workspace-start".to_string(),
            git_url: "https://example.invalid/workspace-start.git".to_string(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: main_branch_ref.map(str::to_string),
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

fn a_workspace_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: PROJECT_ID.to_string(),
        session_type: "workspace".to_string(),
        ..Default::default()
    }
}

/// The worktree a session recorded in its `.session.yaml`.
fn worktree_path_of(sessions_base: &Path, session_id: &str) -> PathBuf {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let metadata =
        tddy_core::read_session_metadata(&session_dir).expect("session metadata must be readable");
    PathBuf::from(
        metadata
            .repo_path
            .expect("a workspace session must record the worktree it created"),
    )
}

/// An agent this daemon defines, taking `Grep` away from whatever main agent it is attached to.
///
/// A seed naming it needs no clone and no peer: it is owned by the daemon under test, so seeding it
/// is complete once the entry is written.
fn an_agent_def_on_this_host(tddy_data_dir: &Path, name: &str) {
    let agents = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://127.0.0.1:11434/v1\nreplaces:\n  - Grep\n"
        ),
    )
    .expect("write agent def");
}

/// The agents a session lists, read the way an operator's Agents tab reads them.
async fn agent_ids_on_the_roster_of(
    service: &tddy_daemon::connection_service::ConnectionServiceImpl,
    session_id: &str,
) -> Vec<String> {
    service
        .list_session_agents(Request::new(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("listing a session's agents must succeed")
        .into_inner()
        .agents
        .into_iter()
        .map(|agent| agent.agent_id)
        .collect()
}

/// The agent half a daemon names when it asks this host to hold a split session's worktree.
fn an_agent_on_another_daemon() -> SplitAgentPlacement {
    SplitAgentPlacement {
        session_id: "019d105b-ac0f-78d3-9a89-409731145b01".to_string(),
        agent_daemon_instance_id: "workstation-b".to_string(),
    }
}

/// The agent session a workspace session recorded as working in its worktree, as
/// `(daemon_instance_id, session_id)`.
///
/// Each half is reported as it was found, rather than collapsed into one `Option`: a pairing is
/// only usable when both are present, so a record carrying one and not the other must read as the
/// distinct thing it is instead of as "no pairing".
fn paired_agent_of(sessions_base: &Path, session_id: &str) -> (Option<String>, Option<String>) {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let metadata =
        tddy_core::read_session_metadata(&session_dir).expect("session metadata must be readable");
    (metadata.agent_daemon_instance_id, metadata.agent_session_id)
}

// ---------------------------------------------------------------------------
// The caller-chosen session id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_session_is_created_under_the_session_id_the_caller_chose() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());
    let chosen = "019d105b-ac0f-78d3-9a89-409731145aaa";

    // When
    let started = service
        .start_session(Request::new(StartSessionRequest {
            requested_session_id: chosen.to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect("a workspace session with a caller-chosen id must start")
        .into_inner();

    // Then — the id the caller recorded before the call is the id the session actually has, which is
    // what makes a forwarded start that never answers still tearable-down
    assert_eq!(
        started.session_id, chosen,
        "the workspace session must be created under the requested id"
    );
    assert!(
        worktree_path_of(sessions_tmp.path(), chosen).exists(),
        "the session created under the chosen id must hold a real worktree"
    );
}

#[tokio::test]
async fn a_caller_chosen_session_id_that_is_already_taken_is_refused() {
    // Given a workspace session already occupying the id
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());
    let chosen = "019d105b-ac0f-78d3-9a89-409731145abb";
    service
        .start_session(Request::new(StartSessionRequest {
            requested_session_id: chosen.to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect("the first workspace session must start");
    let first_worktree = worktree_path_of(sessions_tmp.path(), chosen);

    // When the same id is asked for again
    let status = service
        .start_session(Request::new(StartSessionRequest {
            requested_session_id: chosen.to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect_err("a second session must not be created over an existing one");

    // Then — refused rather than overwriting the live session's metadata, which would leave its
    // worktree with nothing pointing at it
    assert_eq!(
        status.code(),
        tddy_rpc::Code::AlreadyExists,
        "expected AlreadyExists; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        first_worktree.exists(),
        "the existing session's worktree must be untouched at {first_worktree:?}"
    );
}

#[tokio::test]
async fn a_caller_chosen_session_id_is_refused_for_a_session_type_that_does_not_support_it() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When a claude-cli session asks to be created under a chosen id
    let status = service
        .start_session(Request::new(StartSessionRequest {
            session_type: "claude-cli".to_string(),
            model: "claude-opus-5".to_string(),
            requested_session_id: "019d105b-ac0f-78d3-9a89-409731145acc".to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect_err("only workspace sessions accept a caller-chosen id");

    // Then — named, not ignored: a caller that believed it had pinned the id would go on to address
    // a session that does not exist
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("requested_session_id"),
        "the refusal must name the field; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_caller_chosen_session_id_that_is_not_a_safe_path_segment_is_refused() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .start_session(Request::new(StartSessionRequest {
            requested_session_id: "../escape".to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect_err("a traversing session id must be refused");

    // Then — the id becomes a directory name under the sessions base, so it is validated exactly as
    // DeleteSession validates the one it is given
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
}

// ---------------------------------------------------------------------------
// Branch intent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_session_asking_for_a_new_branch_without_a_name_is_refused() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .start_session(Request::new(StartSessionRequest {
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: String::new(),
            ..a_workspace_request()
        }))
        .await
        .expect_err("an unnamed new branch must be refused, as it is for a claude-cli session");

    // Then — the same submission is an error co-located; producing a generated name here instead
    // would hand the caller a branch nobody asked for
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("new_branch_name"),
        "the refusal must name the missing field; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_workspace_session_with_an_unrecognized_branch_intent_is_refused() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .start_session(Request::new(StartSessionRequest {
            branch_worktree_intent: "work_on_selected_brnach".to_string(),
            selected_branch_to_work_on: "release".to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect_err("an intent the daemon does not recognise must be refused");

    // Then — a typo'd intent silently becoming "fresh generated branch" is the worst of both: the
    // session comes up, on a branch that is not the one that was selected
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("work_on_selected_brnach"),
        "the refusal must quote the intent it did not recognise; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_workspace_session_with_no_branch_intent_still_gets_a_generated_branch() {
    // Given — the pre-existing default: a session created purely to hold a checkout
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let started = service
        .start_session(Request::new(a_workspace_request()))
        .await
        .expect("a workspace session with no branch intent must start")
        .into_inner();

    // Then — a blank intent is a genuine absence of intent, not an unrecognised one
    let worktree = worktree_path_of(sessions_tmp.path(), &started.session_id);
    let branch = git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert!(
        branch.starts_with("workspace/"),
        "expected a generated workspace/… branch; got '{branch}'"
    );
}

// ---------------------------------------------------------------------------
// The project's configured default branch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_session_cuts_its_new_branch_from_the_projects_configured_default_branch() {
    // Given a project whose default branch is not the repository's own default
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), Some("origin/release"));
    let service = test_service(sessions_tmp.path().to_path_buf());
    let release_head = git_stdout(repo.path(), &["rev-parse", "release"]);

    // When a session is started without naming a base ref
    let started = service
        .start_session(Request::new(StartSessionRequest {
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: "workspace-from-default".to_string(),
            ..a_workspace_request()
        }))
        .await
        .expect("a workspace session must start")
        .into_inner();

    // Then — the worktree is cut where the co-located claude-cli path would have cut it, rather than
    // from whatever the repository's own default happens to be
    let worktree = worktree_path_of(sessions_tmp.path(), &started.session_id);
    assert_eq!(
        git_stdout(&worktree, &["rev-parse", "HEAD"]),
        release_head,
        "the worktree must start from the project's configured default branch"
    );
}

// ---------------------------------------------------------------------------
// The agent the workspace is paired with
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_session_records_the_agent_session_it_holds_the_worktree_for() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());
    let agent = an_agent_on_another_daemon();

    // When the daemon placing a split session's worktree here names the agent half
    let started = service
        .start_session(Request::new(StartSessionRequest {
            split_agent: Some(agent.clone()),
            ..a_workspace_request()
        }))
        .await
        .expect("a workspace session naming its agent half must start")
        .into_inner();

    // Then — persisted with the session, so the host that runs no agent of its own can still tell
    // that a withdrawal attached to this checkout is enforced somewhere, and where
    assert_eq!(
        paired_agent_of(sessions_tmp.path(), &started.session_id),
        (Some(agent.agent_daemon_instance_id), Some(agent.session_id)),
        "the workspace session must record the agent working in its worktree"
    );
}

#[tokio::test]
async fn a_workspace_session_nobody_named_an_agent_for_is_paired_with_none() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When a checkout is asked for with no agent half named, as an operator's own workspace is
    let started = service
        .start_session(Request::new(a_workspace_request()))
        .await
        .expect("a standalone workspace session must start")
        .into_inner();

    // Then — absent rather than guessed: there is no agent anywhere whose tools a roster attached
    // here could take away, and a withdrawal accepted on it would be enforced by nothing
    assert_eq!(
        paired_agent_of(sessions_tmp.path(), &started.session_id),
        (None, None),
        "a workspace session with no agent named must record no pairing"
    );
}

#[tokio::test]
async fn a_split_agent_placement_is_refused_for_a_session_type_that_does_not_support_it() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When a claude-cli session claims an agent works in its worktree
    let status = service
        .start_session(Request::new(StartSessionRequest {
            session_type: "claude-cli".to_string(),
            model: "claude-opus-5".to_string(),
            split_agent: Some(an_agent_on_another_daemon()),
            ..a_workspace_request()
        }))
        .await
        .expect_err("only workspace sessions hold a worktree for an agent elsewhere");

    // Then — refused rather than dropped: a caller that believed the pairing was recorded would
    // attach agents this session turns out not to enforce
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("split_agent"),
        "the refusal must name the field; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_split_agent_placement_naming_a_daemon_but_no_session_is_refused() {
    // Given
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When the placement names a host but nothing on it
    let status = service
        .start_session(Request::new(StartSessionRequest {
            split_agent: Some(SplitAgentPlacement {
                session_id: String::new(),
                ..an_agent_on_another_daemon()
            }),
            ..a_workspace_request()
        }))
        .await
        .expect_err("half a pairing names nothing that works in the checkout");

    // Then — refused rather than half-recorded, so the pairing the enforcement check reads back is
    // never one nothing answers to
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("split_agent"),
        "the refusal must name the field; got '{}'",
        status.message()
    );
}

// ---------------------------------------------------------------------------
// A start that fails after the roster was seeded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_workspace_start_that_fails_after_seeding_leaves_no_agent_on_the_roster() {
    // Given the codebase half of a split session — the shape that has a paired agent, so a
    // withdrawal is enforceable and the seed goes through — asking for a semantic index the fixture
    // has no embedder for: the seed succeeds, and the step after it is the one that fails
    let repo = a_git_repo_with_two_branches();
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(sessions_tmp.path(), repo.path(), None);
    an_agent_def_on_this_host(sessions_tmp.path(), "fastcontext");
    let service = test_service(sessions_tmp.path().to_path_buf());
    let chosen = "019d105b-ac0f-78d3-9a89-409731145abc";

    // When the start is refused
    service
        .start_session(Request::new(StartSessionRequest {
            requested_session_id: chosen.to_string(),
            split_agent: Some(an_agent_on_another_daemon()),
            specialized_agents: vec!["fastcontext".to_string()],
            semantic_index: true,
            ..a_workspace_request()
        }))
        .await
        .expect_err("a start whose semantic index cannot be built must not report success");

    // Then the roster is empty. A start that answered with an error but left its seed behind leaves
    // an agent the operator can see on a session that never came up, holding a withdrawal against a
    // main agent that was never spawned — and for a peer-owned seed, a claimed clone on that peer
    // with nothing left to release it.
    assert_eq!(
        agent_ids_on_the_roster_of(&service, chosen).await,
        Vec::<String>::new(),
        "a failed start must take its seeded roster back out"
    );
}
