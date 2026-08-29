//! Acceptance: a PR-stack parent is resolved on the host that **owns** it.
//!
//! `StartSessionRequest.stack_parent` is a bare session id, and the daemon that serves the spawn
//! resolves it against its own sessions tree
//! (`resolve_chain_base_ref` → `unified_session_dir_path(sessions_base, stack_parent)`). The
//! new-session form's orchestrator picker is fed by a **fanned-out, all-host** `ListSessions`, so it
//! routinely offers an orchestrator that lives on a different daemon than the one the child is
//! started on — and the spawn is then refused with
//!
//! ```text
//! [failed_precondition] could not resolve stack parent branch: session file missing:
//! parent session not found under sessions tree: tmp/.tddy/sessions/01a04d4b-…
//! ```
//!
//! for a session that exists, one host over. `stack_parent_daemon_instance_id` names the owner, on
//! exactly the routing shape `codebase_daemon_instance_id` and the roster RPCs already use, and
//! `ResolveStackBase` is the question the spawning host asks it: *given this planned branch, what
//! does my child's worktree base off?* The owner answers from its own stack, its own child
//! sessions and its own checkout of the same logical project — the only place all three exist
//! together.
//!
//! No LiveKit and no peer: the daemon is put in a common room with a host it can name, with **no
//! room connected**, so a call that reaches the forwarding path is refused there by
//! `FailedPrecondition` — an observable that only the forwarding path produces. What is under test
//! is which host answers, not what the far side says; the far side is covered here too, by asking
//! the same daemon about a parent it does own.
//!
//! Feature: `docs/ft/coder/pr-stacking.md`

use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::changeset::{write_changeset, Changeset, Stack, StackNode};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ResolveStackBaseRequest, StartSessionRequest,
};

/// The daemon under test: the one a child session is started on, and the one that holds the
/// project. In the reported failure it is also the host with no orchestrator of its own.
const THIS_HOST: &str = "laptop-a";
/// The peer: where the pr-stack orchestrator — its stack, its child sessions — lives.
const ORCHESTRATOR_HOST: &str = "workstation-b";
/// A daemon that is not in this one's common room at all.
const STRANGER_HOST: &str = "nowhere-9";

/// The orchestrator id from the reported failure.
const ORCHESTRATOR_SESSION: &str = "01a04d4b-84f8-7fc0-b020-19ae73981175";

/// The logical project. The same `project_id` on every host that owns it (`AddProjectToHost` reuses
/// it), which is why the owner can resolve *its* checkout from the id the child's host sends.
const PROJECT: &str = "auth-service";

/// The planned node already materialized on a branch — what a descendant bases off.
const PARENT_NODE_BRANCH: &str = "feature/auth/token-store";
/// The branch the child spawn is about to create — how the planned node it belongs to is found.
const CHILD_BRANCH: &str = "feature/auth/middleware";

const VALID_TOKEN: &str = "valid-token";

/// What a forwarded unary call reports when this daemon has no common-room connection — the one
/// observable only the forwarding path produces.
const WENT_TO_THE_PEER: &str =
    "LiveKit common room is not connected on this daemon; cannot forward an RPC to a peer";

/// Today's refusal, and the whole bug: a session that exists elsewhere read as a missing file.
const LOOKED_ON_THIS_HOSTS_DISK: &str = "parent session not found under sessions tree";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The common room this daemon sees: itself and the host holding the orchestrator. `STRANGER_HOST`
/// is deliberately absent, so naming it is a request no routing can satisfy.
struct ACommonRoomWithTheOrchestratorHost;

#[async_trait::async_trait]
impl EligibleDaemonSource for ACommonRoomWithTheOrchestratorHost {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        [THIS_HOST, ORCHESTRATOR_HOST]
            .into_iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.to_string()),
                label: format!("{id} (common room)"),
            })
            .collect()
    }
}

/// A daemon in that common room, serving whatever `_data_dir` holds, with **no room connected**.
struct ADaemonInTheCommonRoom {
    service: ConnectionServiceImpl,
    _data_dir: tempfile::TempDir,
    _repo_dir: tempfile::TempDir,
}

impl ADaemonInTheCommonRoom {
    /// Ask this daemon what `CHILD_BRANCH` bases off, naming the host that owns the stack parent.
    async fn resolve_stack_base_owned_by(
        &self,
        owning_host: &str,
    ) -> Result<String, tddy_rpc::Status> {
        self.service
            .resolve_stack_base(Request::new(ResolveStackBaseRequest {
                session_token: VALID_TOKEN.to_string(),
                daemon_instance_id: owning_host.to_string(),
                stack_parent: ORCHESTRATOR_SESSION.to_string(),
                project_id: PROJECT.to_string(),
                new_branch_name: CHILD_BRANCH.to_string(),
            }))
            .await
            .map(|r| r.into_inner().base_ref)
    }

    /// Start a child session for `CHILD_BRANCH` under an orchestrator owned by `owning_host`.
    async fn start_child_session_under_an_orchestrator_on(
        &self,
        owning_host: &str,
    ) -> Result<(), tddy_rpc::Status> {
        self.service
            .start_session(Request::new(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: PROJECT.to_string(),
                session_type: "claude-cli".to_string(),
                model: "anthropic/claude-haiku-4-5".to_string(),
                branch_worktree_intent: "new_branch_from_base".to_string(),
                new_branch_name: CHILD_BRANCH.to_string(),
                stack_parent: ORCHESTRATOR_SESSION.to_string(),
                stack_parent_daemon_instance_id: owning_host.to_string(),
                ..Default::default()
            }))
            .await
            .map(|_| ())
    }
}

/// The OS user the test process runs as — the config must map a real user, because a request that
/// got past the refusal under test would spawn as that user.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A daemon in the common room, holding the project's repository and whatever sessions
/// `write_sessions` puts in its sessions tree, with **no room connected**.
///
/// LiveKit is configured even though nothing here connects: the credentials are resolved from
/// config on the way through `StartSession`, and a daemon without them refuses for that reason
/// instead of the one under test.
fn a_daemon_in_the_common_room(write_sessions: impl Fn(&Path, &Path)) -> ADaemonInTheCommonRoom {
    let data_dir = tempfile::tempdir().expect("data dir");
    let repo_dir = tempfile::tempdir().expect("repo dir");
    a_repo_with_origin_master_and_a_pushed(repo_dir.path(), PARENT_NODE_BRANCH);
    register_project(&data_dir.path().join("projects"), repo_dir.path());
    write_sessions(data_dir.path(), repo_dir.path());

    let user = current_os_user();
    let config_yaml = format!(
        r#"
daemon_instance_id: "{THIS_HOST}"
users:
  - github_user: "{user}"
    os_user: "{user}"
livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: devsecret
"#
    );
    let config_path = data_dir.path().join("daemon.yaml");
    std::fs::write(&config_path, config_yaml).expect("write daemon config");
    let config = DaemonConfig::load(&config_path).expect("load daemon config");

    let base = data_dir.path().to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));

    let service = ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        data_dir.path().to_path_buf(),
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: Arc::new(ACommonRoomWithTheOrchestratorHost),
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(CliSessionManager::new()),
    );

    ADaemonInTheCommonRoom {
        service,
        _data_dir: data_dir,
        _repo_dir: repo_dir,
    }
}

/// The spawning host of the reported failure: it holds the project, and no orchestrator at all.
fn a_host_that_owns_no_orchestrator() -> ADaemonInTheCommonRoom {
    a_daemon_in_the_common_room(|_data_dir, _repo| {})
}

/// A host that owns the orchestrator itself: `token-store` is materialized on its branch, and
/// `middleware` is the planned node the child spawn belongs to.
fn a_host_that_owns_the_orchestrator() -> ADaemonInTheCommonRoom {
    a_daemon_in_the_common_room(|data_dir, repo| {
        a_session(
            data_dir,
            ORCHESTRATOR_SESSION,
            Changeset {
                recipe: Some("pr-stack".to_string()),
                repo_path: Some(repo.display().to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![
                        a_materialized_node("token-store", PARENT_NODE_BRANCH),
                        a_planned_node("middleware", CHILD_BRANCH, &["token-store"]),
                    ],
                }),
                ..Changeset::default()
            },
        );
    })
}

/// A host that owns the parent as an ordinary **code session** on a branch — single-level chaining,
/// the other shape `stack_parent` carries.
fn a_host_that_owns_the_parent_code_session() -> ADaemonInTheCommonRoom {
    a_daemon_in_the_common_room(|data_dir, repo| {
        a_session(
            data_dir,
            ORCHESTRATOR_SESSION,
            Changeset {
                recipe: Some("tdd".to_string()),
                branch: Some(PARENT_NODE_BRANCH.to_string()),
                repo_path: Some(repo.display().to_string()),
                ..Changeset::default()
            },
        );
    })
}

/// A host that owns the orchestrator, whose stack has no node for the branch the child creates.
fn a_host_whose_orchestrator_planned_no_such_branch() -> ADaemonInTheCommonRoom {
    a_daemon_in_the_common_room(|data_dir, repo| {
        a_session(
            data_dir,
            ORCHESTRATOR_SESSION,
            Changeset {
                recipe: Some("pr-stack".to_string()),
                repo_path: Some(repo.display().to_string()),
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![a_materialized_node("token-store", PARENT_NODE_BRANCH)],
                }),
                ..Changeset::default()
            },
        );
    })
}

/// A node whose child worktree exists — it owns a real branch, so descendants can base onto it.
fn a_materialized_node(node_id: &str, branch: &str) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        branch_suggestion: Some(branch.to_string()),
        branch: Some(branch.to_string()),
        ..StackNode::default()
    }
}

/// A node the planner wrote and nothing has spawned yet: a suggested branch and no branch.
fn a_planned_node(node_id: &str, branch_suggestion: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        branch_suggestion: Some(branch_suggestion.to_string()),
        parents: parents.iter().map(|p| (*p).to_string()).collect(),
        ..StackNode::default()
    }
}

fn a_session(data_dir: &Path, session_id: &str, changeset: Changeset) {
    let dir = unified_session_dir_path(data_dir, session_id);
    std::fs::create_dir_all(&dir).expect("create session dir");
    write_changeset(&dir, &changeset).expect("write changeset");
}

/// A repository with a real `origin/master` and `branch` pushed to it: the base resolver fetches the
/// remote and reads remote-tracking refs, so a repo with no pushed refs fails for its own reason.
fn a_repo_with_origin_master_and_a_pushed(dir: &Path, branch: &str) {
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git must run");
        assert!(
            out.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-b", "master"]);
    git(&["commit", "--allow-empty", "-m", "init"]);
    git(&["remote", "add", "origin", dir.to_str().unwrap()]);
    git(&["push", "-u", "origin", "master"]);
    git(&["checkout", "-b", branch]);
    git(&["commit", "--allow-empty", "-m", "parent node"]);
    git(&["push", "-u", "origin", branch]);
    git(&["checkout", "master"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).expect("create projects dir");
    let yaml = format!(
        "projects:\n  - project_id: {PROJECT}\n    name: auth-service\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().expect("repo path is utf-8")
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).expect("write projects.yaml");
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait StackParentAssertions {
    /// The call left this daemon for the named host — the only path that reports `WENT_TO_THE_PEER`.
    fn assert_asked_the_owning_host(&self) -> &Self;
    /// The call named a host that cannot serve it, and was refused as a bad request.
    fn assert_refused_as_unroutable(&self, host: &str) -> &Self;
    /// The call was answered off this daemon's own disk.
    fn assert_looked_on_this_hosts_disk(&self) -> &Self;
}

impl<T> StackParentAssertions for Result<T, tddy_rpc::Status> {
    fn assert_asked_the_owning_host(&self) -> &Self {
        let status = self
            .as_ref()
            .err()
            .expect("a call forwarded with no room connected must fail, not be served locally");
        assert_eq!(
            (status.code(), status.message()),
            (Code::FailedPrecondition, WENT_TO_THE_PEER),
            "the stack parent was not resolved on the host that owns it"
        );
        self
    }

    fn assert_refused_as_unroutable(&self, host: &str) -> &Self {
        let status = self
            .as_ref()
            .err()
            .expect("a call naming a host outside the common room must fail");
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(
            status.message().contains(host),
            "the refusal must name the host that cannot serve the call, got: {}",
            status.message()
        );
        self
    }

    fn assert_looked_on_this_hosts_disk(&self) -> &Self {
        let status = self
            .as_ref()
            .err()
            .expect("a parent no host holds must fail");
        assert!(
            status.message().contains(LOOKED_ON_THIS_HOSTS_DISK),
            "expected the local sessions-tree refusal, got: {}",
            status.message()
        );
        self
    }
}

// ---------------------------------------------------------------------------
// The spawning host asks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_stack_parent_owned_by_another_host_is_resolved_there() {
    // Given a host that holds the project but no orchestrator of its own
    let daemon = a_host_that_owns_no_orchestrator();

    // When it is asked for a base, naming the host that owns the stack
    let answer = daemon.resolve_stack_base_owned_by(ORCHESTRATOR_HOST).await;

    // Then it asks that host rather than reading a sessions tree that cannot hold the answer
    answer.assert_asked_the_owning_host();
}

#[tokio::test]
async fn a_stack_parent_named_on_a_host_outside_the_common_room_is_refused() {
    // Given the same host, and an owner id no routing can satisfy
    let daemon = a_host_that_owns_no_orchestrator();

    // When
    let answer = daemon.resolve_stack_base_owned_by(STRANGER_HOST).await;

    // Then the caller named a host that cannot serve the call — a bad request, not a base
    answer.assert_refused_as_unroutable(STRANGER_HOST);
}

#[tokio::test]
async fn a_stack_parent_no_host_was_named_for_is_still_looked_for_on_this_one() {
    // Given a host that owns no orchestrator, asked the way every pre-existing caller asks
    let daemon = a_host_that_owns_no_orchestrator();

    // When
    let answer = daemon.resolve_stack_base_owned_by("").await;

    // Then single-host deployments keep today's answer: this daemon's own sessions tree
    answer.assert_looked_on_this_hosts_disk();
}

// ---------------------------------------------------------------------------
// The owning host answers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_owning_host_answers_a_planned_nodes_base_from_its_own_stack() {
    // Given the host that holds the orchestrator, its stack, and its checkout of the project
    let daemon = a_host_that_owns_the_orchestrator();

    // When it is asked, as the owner, what the planned `middleware` branch bases off
    let base = daemon
        .resolve_stack_base_owned_by(THIS_HOST)
        .await
        .expect("the host that owns the orchestrator must answer");

    // Then the nearest materialized ancestor's branch — what a stacked PR is opened against
    assert_eq!(base, format!("origin/{PARENT_NODE_BRANCH}"));
}

#[tokio::test]
async fn the_owning_host_answers_a_code_session_parents_own_branch() {
    // Given the host that holds a plain code session on a branch as the parent
    let daemon = a_host_that_owns_the_parent_code_session();

    // When
    let base = daemon
        .resolve_stack_base_owned_by(THIS_HOST)
        .await
        .expect("the host that owns the parent session must answer");

    // Then single-level chaining bases the child off the parent's pushed branch
    assert_eq!(base, format!("origin/{PARENT_NODE_BRANCH}"));
}

#[tokio::test]
async fn the_owning_host_answers_an_empty_base_when_its_stack_planned_no_such_branch() {
    // Given an orchestrator whose stack has no node for the branch the child creates
    let daemon = a_host_whose_orchestrator_planned_no_such_branch();

    // When
    let base = daemon
        .resolve_stack_base_owned_by(THIS_HOST)
        .await
        .expect("an unplanned branch is not an error — it has no chain base");

    // Then the empty ref says "no chain base", and the child's host applies the project default
    assert_eq!(base, "");
}

// ---------------------------------------------------------------------------
// The wiring: StartSession reads the field
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_session_resolves_a_remote_stack_parent_on_the_host_that_owns_it() {
    // Given the reported case: a child started on the host that holds the project, under an
    // orchestrator that lives on another host
    let daemon = a_host_that_owns_no_orchestrator();

    // When
    let started = daemon
        .start_child_session_under_an_orchestrator_on(ORCHESTRATOR_HOST)
        .await;

    // Then the spawn asks the owning host — every test above would still pass with
    // `stack_parent_daemon_instance_id` dropped on the floor, and this is the one that would not
    started.assert_asked_the_owning_host();
}
