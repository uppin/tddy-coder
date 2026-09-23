//! Acceptance tests: the **sandboxed codebase** placement — this daemon jails its own checkout in
//! a `--workspace-tools` jail and runs the agent beside it, unconfined, with every native
//! filesystem and shell tool withdrawn.
//!
//! It is the third placement, not a shade of the other two. `CoLocated` and `Split` differ only in
//! *which host* holds the checkout; both leave the jail question to `sandbox`, which jails the
//! **agent**. This one inverts that: the code is confined and the agent is not.
//!
//! Implemented as the split orchestration with the peer hop removed — a **local** `workspace`
//! session with `sandbox: Some(true)` holds the worktree, and the agent's `mcp__tddy-tools__*`
//! calls address *that* session, so `exec_tool_route`'s existing predicate
//! (`session_type == "workspace" && sandbox == Some(true)`) lands them in the jail unchanged.
//!
//! These tests need **no common room**, and one of them asserts exactly that: a placement whose
//! whole point is that both halves are on this host must not require one. The fixture config has
//! no `livekit:` block at all — the daemon signs its session tokens with a key of its own.
//!
//! Everything here is asserted through the RPC surface or through what the daemon persisted —
//! `StartSession`, `ExecuteTool`, and `.session.yaml`. The agent's argv and its `TDDY_REMOTE_*`
//! environment are `pub(crate)` details and are pinned by unit tests inside
//! `tddy-session-lifecycle`, not by a test-only `pub` opened for this suite.
//!
//! Subject crate: `tddy-session-lifecycle` (the placement and the start path), with
//! `tddy-daemon-sandbox` consumed unchanged. Filed here beside the existing sandbox suites — see
//! `packages/tddy-daemon/docs/code-issues/misplaced-tests-integration-suites.md` § Concurrent
//! changes, which routes it.
//!
//! PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
//! Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_github::GitHubUser;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::Request;
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::{
    classify_placement, CodebasePlacement, DaemonSessionHost, PlacementRequest,
};
use tddy_session_lifecycle::test_util::TestDaemon;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const LOCAL_INSTANCE_ID: &str = "workstation";
const A_PEER_ID: &str = "laptop-b";
/// A specialized agent this deployment offers, as the create-session form would name it.
const A_SPECIALIZED_AGENT: &str = "fastcontext";
const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a36";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct MockEligibleDaemonSource {
    ids: Vec<String>,
}

impl EligibleDaemonSource for MockEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        self.ids
            .iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.clone()),
                label: id.clone(),
            })
            .collect()
    }
}

/// This daemon's signing identity: the key the browser's credential below is signed with, and the
/// one the daemon mints the agent a credential of its own with for the tool calls it makes back
/// here. A daemon given none refuses the start rather than forwarding the caller's token, so every
/// test below would exercise that refusal. Generated once per test process, so the suite has one
/// identity across its tests and no run inherits another's key; the key file's directory is
/// dropped as soon as the key is in memory.
fn this_daemons_session_tokens() -> &'static SessionTokens {
    static TOKENS: OnceLock<SessionTokens> = OnceLock::new();
    TOKENS.get_or_init(|| {
        let home = tempfile::tempdir().expect("a directory for the daemon's key");
        let key = DaemonSigningKey::load_or_generate(&home.path().join("signing_key.pem"))
            .expect("the daemon generates its keypair");
        SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory))
    })
}

/// The credential the browser presents, signed with [`this_daemons_session_tokens`]'s key so this daemon can verify
/// it and mint the agent's own from the identity it proves. Minted once and shared, because the
/// request and the daemon's user resolver have to agree on the very same string.
fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        this_daemons_session_tokens()
            .signer()
            .mint_access(&GitHubUser {
                id: 4242,
                login: current_os_user(),
                avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
                name: "Test User".to_string(),
            })
    })
}

/// The OS user this test process runs as — a real, resolvable one. A fabricated name does not
/// resolve during the claude-cli spawn, and these are the first sandboxed-codebase suites that
/// actually complete one. Same pattern as `claude_cli_session_acceptance.rs`.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

/// A repository with a committed `README.md` — the file the tool calls below read. An empty repo
/// would let a jail answer "file not found" and the routing assertion would pass for the wrong
/// reason.
fn a_git_repo_with_origin_at(path: &Path) {
    // Idempotent: a restart test stands a second daemon on the same sessions base, which would
    // otherwise re-run `git commit` on a tree with nothing to commit.
    if path.join(".git").is_dir() {
        return;
    }
    std::fs::create_dir_all(path).expect("create fixture repo dir");
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    std::fs::write(path.join("README.md"), "# fixture\n").expect("write README");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-q", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_projects::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_projects::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "sandboxed-codebase".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

fn test_config() -> DaemonConfig {
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
daemon_instance_id: "{LOCAL_INSTANCE_ID}"
claude_cli:
  binary_path: /bin/cat
"#
    );
    serde_yaml::from_str(&yaml).expect("config must parse")
}

fn user_resolver_valid() -> UserResolver {
    Arc::new(|token| {
        if token == a_caller_token() {
            Some(current_os_user())
        } else {
            None
        }
    })
}

/// Define one specialized agent on this daemon, under `<tddyhome>/agents`.
///
/// Part of the fixture deployment rather than of a single test: a start naming an agent no host
/// defines is refused by name (`seeded_roster_records`), so without a def the managed cases below
/// would exercise that refusal instead of the placement.
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

fn a_daemon(sessions_base: PathBuf, discovery: Option<LiveKitDiscoveryHandles>) -> TestDaemon {
    an_agent_def_on_this_host(&sessions_base, A_SPECIALIZED_AGENT);
    let repo = sessions_base.join("fixture-repo");
    a_git_repo_with_origin_at(&repo);
    register_project(&sessions_base, &repo);
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    TestDaemon::from_arc(Arc::new(
        DaemonSessionHost::new(
            test_config(),
            resolver,
            sessions_base,
            user_resolver_valid(),
            None,
            discovery,
            None,
            Arc::new(ClaudeCliSessionManager::new()),
        )
        .with_session_tokens(this_daemons_session_tokens().clone()),
    ))
}

/// A daemon with **no LiveKit discovery at all** — no eligible-peer source, no common room.
///
/// This is the fixture that matters for this placement: both halves live here, so a daemon that
/// has never joined a room must still serve it. A fixture carrying discovery handles would let a
/// hidden dependency on peer discovery pass unnoticed.
///
/// Its config has no `livekit:` block at all. What lets it mint the agent a credential of its own
/// instead of refusing the start is its own signing identity ([`this_daemons_session_tokens`]).
fn a_daemon_with_no_common_room(sessions_base: PathBuf) -> TestDaemon {
    a_daemon(sessions_base, None)
}

/// The same daemon, but aware of one eligible peer — the fixture for asserting that asking for
/// both this placement and a split placement is refused rather than silently resolved to either.
fn a_daemon_that_knows_a_peer(sessions_base: PathBuf) -> TestDaemon {
    a_daemon(
        sessions_base,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: Arc::new(MockEligibleDaemonSource {
                ids: vec![A_PEER_ID.to_string()],
            }) as Arc<dyn EligibleDaemonSource>,
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
    )
}

/// A claude-cli start request asking for the jailed-codebase placement and nothing else.
fn a_sandboxed_codebase_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: a_caller_token().to_string(),
        project_id: PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        sandboxed_codebase: true,
        ..Default::default()
    }
}

/// A jailed-codebase session that also runs the managed workflow, with one specialized agent.
/// `recipe` stays empty on purpose: a recipe resolves `TDDY_REPO_DIR` where the *agent* is rather
/// than where the code is, so it remains refused on this placement. The roster does not need one.
fn a_managed_sandboxed_codebase_request() -> StartSessionRequest {
    StartSessionRequest {
        managed_codebase: true,
        specialized_agents: vec![A_SPECIALIZED_AGENT.to_string()],
        ..a_sandboxed_codebase_request()
    }
}

/// The placement question as this daemon asks it, with nothing chosen.
fn a_placement_request() -> PlacementRequest {
    PlacementRequest {
        local_instance_id: LOCAL_INSTANCE_ID.to_string(),
        requested_codebase_id: String::new(),
        eligible_ids: vec![A_PEER_ID.to_string()],
        managed_codebase: false,
        sandbox: false,
        sandboxed_codebase: false,
        session_type: "claude-cli".to_string(),
        recipe: String::new(),
        dangerously_skip_permissions: false,
    }
}

fn a_jailed_codebase_placement_request() -> PlacementRequest {
    PlacementRequest {
        sandboxed_codebase: true,
        ..a_placement_request()
    }
}

fn metadata_of(sessions_base: &Path, session_id: &str) -> SessionMetadata {
    read_session_metadata(&unified_session_dir_path(sessions_base, session_id))
        .unwrap_or_else(|e| panic!("session {session_id} must have readable metadata: {e}"))
}

/// The workspace session holding a jailed-codebase session's checkout.
fn checkout_session_of(sessions_base: &Path, agent_session_id: &str) -> String {
    metadata_of(sessions_base, agent_session_id)
        .codebase_session_id
        .expect("the agent half must record the workspace session holding its checkout")
}

// ---------------------------------------------------------------------------
// classify_placement — which of three placements a request asks for
// ---------------------------------------------------------------------------

#[test]
fn a_sandboxed_codebase_request_is_classified_as_its_own_placement() {
    // Given a claude-cli request asking for the codebase to be jailed
    let request = a_jailed_codebase_placement_request();

    // When
    let placement = classify_placement(&request);

    // Then it is the third placement, not a co-located session that happens to carry a flag
    assert_eq!(placement, Ok(CodebasePlacement::SandboxedCodebase));
}

#[test]
fn an_empty_codebase_host_is_still_co_located() {
    // Given a request naming no codebase host and asking for no jail
    let request = a_placement_request();

    // When
    let placement = classify_placement(&request);

    // Then — the rule every session created before this feature depends on
    assert_eq!(placement, Ok(CodebasePlacement::CoLocated));
}

#[test]
fn naming_this_daemon_as_the_codebase_host_is_still_co_located() {
    // Given a request that spells "same as host" explicitly
    let request = PlacementRequest {
        requested_codebase_id: LOCAL_INSTANCE_ID.to_string(),
        managed_codebase: true,
        ..a_placement_request()
    };

    // When
    let placement = classify_placement(&request);

    // Then — naming your own daemon is the explicit spelling of co-located. Folding this into the
    // new placement would silently jail the checkout of every session that spells it that way.
    assert_eq!(placement, Ok(CodebasePlacement::CoLocated));
}

#[test]
fn a_known_peer_is_still_a_split_placement() {
    // Given a managed claude-cli request naming an eligible peer
    let request = PlacementRequest {
        requested_codebase_id: A_PEER_ID.to_string(),
        managed_codebase: true,
        ..a_placement_request()
    };

    // When
    let placement = classify_placement(&request);

    // Then — the cross-host inversion is untouched by the co-located one
    assert_eq!(
        placement,
        Ok(CodebasePlacement::Split {
            codebase_instance_id: A_PEER_ID.to_string(),
        })
    );
}

#[test]
fn a_jailed_codebase_session_may_also_be_managed() {
    // Given a request to jail the codebase AND to run the managed workflow over it.
    // `managed_codebase` is not a placement: it carries a recipe, a specialized-agent roster and a
    // per-session toolcall listener, and confines nothing. The flag it was once refused beside is
    // `sandbox`, which jails the agent — that one is still an opposite placement, and still
    // refused below.
    let request = PlacementRequest {
        managed_codebase: true,
        ..a_jailed_codebase_placement_request()
    };

    // When
    let placement = classify_placement(&request);

    // Then the codebase is still jailed — orchestration did not change where anything runs
    assert_eq!(placement, Ok(CodebasePlacement::SandboxedCodebase));
}

#[test]
fn a_sandboxed_codebase_request_with_the_agent_sandbox_is_refused_naming_both_placements() {
    // Given a request asking to jail the code and the agent
    let request = PlacementRequest {
        sandbox: true,
        ..a_jailed_codebase_placement_request()
    };

    // When
    let error = classify_placement(&request)
        .expect_err("jailing the code and jailing the agent are opposite placements");

    // Then the message names the flag that was *also* set, which is the thing the caller has to
    // drop. "sandbox" alone is a substring of "sandboxed_codebase", so it could never fail on its
    // own — the refusal has to be readable as being about this pair specifically.
    assert!(
        error.contains("mutually exclusive with sandbox:"),
        "the refusal must name the agent sandbox as the other placement; got '{error}'"
    );
}

#[test]
fn a_sandboxed_codebase_request_with_a_codebase_host_is_refused_naming_the_split() {
    // Given a request asking for the same inversion twice — once here, once across two hosts
    let request = PlacementRequest {
        requested_codebase_id: A_PEER_ID.to_string(),
        managed_codebase: true,
        ..a_jailed_codebase_placement_request()
    };

    // When
    let error = classify_placement(&request)
        .expect_err("the co-located and cross-host inversions are one choice, not two");

    // Then the message points at the split as the cross-host form
    assert!(
        error.contains("codebase_daemon_instance_id"),
        "the refusal must name the field that already serves the cross-host form; got '{error}'"
    );
}

#[test]
fn a_sandboxed_codebase_request_for_cursor_cli_is_refused_naming_the_withdrawable_tool_surface() {
    // Given a cursor-cli request asking for the codebase to be jailed
    let request = PlacementRequest {
        session_type: "cursor-cli".to_string(),
        ..a_jailed_codebase_placement_request()
    };

    // When
    let error = classify_placement(&request)
        .expect_err("cursor-agent has no --disallowedTools, so this would confine nothing");

    // Then
    assert!(
        error.contains("cursor-cli"),
        "the refusal must name the offending session type; got '{error}'"
    );
}

#[test]
fn a_sandboxed_codebase_request_for_a_tool_session_is_refused_naming_the_session_type() {
    // Given a tddy-coder tool session asking for the codebase to be jailed
    let request = PlacementRequest {
        session_type: "tool".to_string(),
        ..a_jailed_codebase_placement_request()
    };

    // When
    let error = classify_placement(&request).expect_err("only claude-cli can be confined this way");

    // Then the *session type* is named, not merely the word "tool" that the refusal's own
    // explanation ("tool surface") contains whatever type was asked for
    assert!(
        error.contains("\"tool\""),
        "the refusal must name the offending session type; got '{error}'"
    );
}

#[test]
fn a_jailed_codebase_session_may_skip_its_permission_prompts() {
    // Given a request to jail the codebase and to let the agent skip its permission prompts.
    //
    // This placement confines in two layers: the kernel jail around the checkout, and the
    // withdrawn-tool deny list that forces every codebase access through `mcp__tddy-tools__*`.
    // The flag bypasses the second and cannot touch the first, so the trade is real but bounded —
    // the operator gives up "only through the routed tool surface", not "only inside the jail".
    // Stated in the PRD rather than enforced by a refusal.
    let request = PlacementRequest {
        dangerously_skip_permissions: true,
        ..a_jailed_codebase_placement_request()
    };

    // When
    let placement = classify_placement(&request);

    // Then the placement is unchanged — the jail is what the placement is, and the flag is not
    // an argument about where the code lives
    assert_eq!(placement, Ok(CodebasePlacement::SandboxedCodebase));
}

#[test]
fn a_sandboxed_codebase_request_carrying_a_recipe_is_refused() {
    // Given a request asking for a workflow recipe on a jailed checkout
    let request = PlacementRequest {
        recipe: "tdd".to_string(),
        ..a_jailed_codebase_placement_request()
    };

    // When
    let error = classify_placement(&request)
        .expect_err("a recipe resolves TDDY_REPO_DIR where the agent is, not where the code is");

    // Then
    assert!(
        error.contains("recipe"),
        "the refusal must name the field that cannot be honoured; got '{error}'"
    );
}

// ---------------------------------------------------------------------------
// StartSession — what the placement persists
// ---------------------------------------------------------------------------

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_start_places_the_worktree_in_a_local_workspace_session() {
    // Given a daemon with no LiveKit
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When a session asks for its codebase to be jailed
    let started = service
        .start_session(Request::direct(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();

    // Then a local workspace session holds the worktree, recorded as sandboxed
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let workspace = metadata_of(sessions_tmp.path(), &checkout);
    assert_eq!(workspace.session_type.as_deref(), Some("workspace"));
    assert_eq!(workspace.sandbox, Some(true));

    // Teardown. A jail is a real `tddy-sandbox-runner` child in its own process group, reparented
    // to the init process when this test binary exits, so a test that merely stops naming it
    // leaves it on the host for as long as the machine is up. `shut_down_children` is the
    // production shutdown path (the daemon's SIGTERM handler calls it), driven here so the test
    // says where its jail dies; `WorkspaceSandboxRegistry`'s own `Drop` is what covers the
    // panicking exit above, where this line is never reached.
    service.shut_down_children().await;
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_start_pairs_its_checkout_with_this_daemon() {
    // Given a daemon with no LiveKit
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When a session asks for its codebase to be jailed
    let started = service
        .start_session(Request::direct(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();

    // Then the pairing names this daemon, so `split_pairing` reads it and resume and delete
    // follow it exactly as they do for a two-host split
    let agent = metadata_of(sessions_tmp.path(), &started.session_id);
    assert_eq!(
        agent.codebase_daemon_instance_id.as_deref(),
        Some(LOCAL_INSTANCE_ID)
    );

    // Teardown. A jail is a real `tddy-sandbox-runner` child in its own process group, reparented
    // to the init process when this test binary exits, so a test that merely stops naming it
    // leaves it on the host for as long as the machine is up. `shut_down_children` is the
    // production shutdown path (the daemon's SIGTERM handler calls it), driven here so the test
    // says where its jail dies; `WorkspaceSandboxRegistry`'s own `Drop` is what covers the
    // panicking exit above, where this line is never reached.
    service.shut_down_children().await;
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_start_leaves_its_agent_unjailed() {
    // Given a daemon with no LiveKit
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When a session asks for its codebase to be jailed
    let started = service
        .start_session(Request::direct(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();

    // Then the agent half is not sandboxed — confining it is the placement this one inverts
    let agent = metadata_of(sessions_tmp.path(), &started.session_id);
    assert_eq!(agent.sandbox, None);
    assert_eq!(agent.session_type.as_deref(), Some("claude-cli"));

    // Teardown. A jail is a real `tddy-sandbox-runner` child in its own process group, reparented
    // to the init process when this test binary exits, so a test that merely stops naming it
    // leaves it on the host for as long as the machine is up. `shut_down_children` is the
    // production shutdown path (the daemon's SIGTERM handler calls it), driven here so the test
    // says where its jail dies; `WorkspaceSandboxRegistry`'s own `Drop` is what covers the
    // panicking exit above, where this line is never reached.
    service.shut_down_children().await;
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_session_starts_with_no_common_room_configured() {
    // Given a daemon that has never joined a common room
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When a session asks for its codebase to be jailed
    let started = service
        .start_session(Request::direct(a_sandboxed_codebase_request()))
        .await
        .expect("both halves are on this host, so no room is needed to pair them")
        .into_inner();

    // Then no room was opened for it — the split path's LiveKit wiring is not on this one
    assert_eq!(
        started.livekit_room, "",
        "a placement whose halves share a host must not require a room to pair them"
    );

    // Teardown. A jail is a real `tddy-sandbox-runner` child in its own process group, reparented
    // to the init process when this test binary exits, so a test that merely stops naming it
    // leaves it on the host for as long as the machine is up. `shut_down_children` is the
    // production shutdown path (the daemon's SIGTERM handler calls it), driven here so the test
    // says where its jail dies; `WorkspaceSandboxRegistry`'s own `Drop` is what covers the
    // panicking exit above, where this line is never reached.
    service.shut_down_children().await;
}

// ---------------------------------------------------------------------------
// ExecuteTool — the code is reached through the jail, or not at all
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// StartSession — the refusals, at the request level
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_session_refuses_a_jailed_codebase_alongside_a_codebase_host() {
    // Given a daemon that knows an eligible peer
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_that_knows_a_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        managed_codebase: true,
        codebase_daemon_instance_id: A_PEER_ID.to_string(),
        ..a_sandboxed_codebase_request()
    };

    // When
    let status = service
        .start_session(Request::direct(request))
        .await
        .expect_err("asking for both forms of the same inversion must be refused");

    // Then it is a malformed request, not a silently chosen placement
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("codebase_daemon_instance_id"),
        "the refusal must name the field that already serves the cross-host form; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn start_session_refuses_a_jailed_codebase_alongside_the_agent_sandbox() {
    // Given a request asking to jail both the code and the agent
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        sandbox: true,
        ..a_sandboxed_codebase_request()
    };

    // When
    let status = service
        .start_session(Request::direct(request))
        .await
        .expect_err("opposite placements must be refused, not resolved to either");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status
            .message()
            .contains("mutually exclusive with sandbox:"),
        "the refusal must name the agent sandbox as the other placement; got '{}'",
        status.message()
    );
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_jailed_codebase_session_that_skips_permissions_is_still_jailed() {
    // Given a request asking to jail the codebase and to bypass the agent's permission prompts
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        dangerously_skip_permissions: true,
        ..a_sandboxed_codebase_request()
    };

    // When
    let started = service
        .start_session(Request::direct(request))
        .await
        .expect("the bypass is the operator's to take; it is not a reason to refuse the placement")
        .into_inner();

    // Then the kernel jail is still provisioned, which is the half of the confinement the flag
    // cannot reach. What it does surrender — that every codebase access goes through the routed
    // tool surface — is stated in the PRD rather than enforced by a refusal here.
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let workspace = metadata_of(sessions_tmp.path(), &checkout);
    assert_eq!(workspace.sandbox, Some(true));

    service.shut_down_children().await;
}

#[tokio::test]
async fn start_session_refuses_a_jailed_codebase_on_a_cursor_cli_session() {
    // Given a cursor-cli request asking for the codebase to be jailed
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        session_type: "cursor-cli".to_string(),
        ..a_sandboxed_codebase_request()
    };

    // When
    let status = service
        .start_session(Request::direct(request))
        .await
        .expect_err("cursor-agent's tool surface cannot be withdrawn, so this confines nothing");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("cursor-cli"),
        "the refusal must name the offending session type; got '{}'",
        status.message()
    );
}

// ---------------------------------------------------------------------------
// The managed workflow over a jailed codebase
// ---------------------------------------------------------------------------

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn managing_a_jailed_codebase_session_leaves_its_checkout_in_the_jail() {
    // Given a daemon with no LiveKit
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When a session asks for its codebase to be jailed AND for the managed workflow over it
    let started = service
        .start_session(Request::direct(a_managed_sandboxed_codebase_request()))
        .await
        .expect("orchestration and confinement are orthogonal, so both must be served")
        .into_inner();

    // Then the checkout is still jailed — managing a session moved nothing
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let workspace = metadata_of(sessions_tmp.path(), &checkout);
    assert_eq!(workspace.session_type.as_deref(), Some("workspace"));
    assert_eq!(workspace.sandbox, Some(true));

    // And the agent half holds no roster of its own. One half keeps it — the codebase half, which
    // is where a roster agent's own loop reads its withdrawals from — and a second copy here would
    // be stale from the first attach or detach onward. The roster itself is asserted on the half
    // that owns it, in the test below.
    let agent = metadata_of(sessions_tmp.path(), &started.session_id);
    assert_eq!(agent.session_type.as_deref(), Some("claude-cli"));
    assert!(
        agent.agents.is_empty(),
        "the agent half must not carry a second copy of the roster; got {:?}",
        agent.agents
    );

    service.shut_down_children().await;
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_specialized_agent_on_a_jailed_codebase_session_is_held_by_the_codebase_half() {
    // Given a managed, jailed-codebase session
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::direct(a_managed_sandboxed_codebase_request()))
        .await
        .expect("a managed jailed-codebase session must start")
        .into_inner();

    // When the roster is read off the half that holds it. A split session's roster lives on the
    // codebase half — `rosterHalfOf` maps a session to `codebase_session_id` — and this placement
    // is that split with the peer hop removed, so the same rule decides which half answers.
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let workspace = metadata_of(sessions_tmp.path(), &checkout);

    // Then the agent the session was started with is on that half, addressable by the id the main
    // agent uses. Until now no subagent could be reached on this placement at all: a roster needs
    // a full RPC client to the daemon, and the HTTP relay this placement was given was never one.
    let ids: Vec<&str> = workspace
        .agents
        .iter()
        .map(|a| a.agent_id.as_str())
        .collect();
    assert!(
        ids.iter().any(|id| id.starts_with(A_SPECIALIZED_AGENT)),
        "the codebase half must hold the roster the session was started with; got {ids:?}"
    );

    service.shut_down_children().await;
}
