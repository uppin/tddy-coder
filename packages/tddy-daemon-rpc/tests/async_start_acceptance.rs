//! Acceptance: `StartSession` returns while the session is still being started; the orchestrator
//! polls the start with `GetSessionStart` and can stop it with `CancelSessionStart`
//! (`#start-parity` 4/9).
//!
//! The project's repository carries a `post-checkout` hook that blocks until the test releases it,
//! so the start is held inside its worktree step for as long as a test needs.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-async-start.md

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use support::{
    a_host, a_workspace_in, git_stdout, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT,
};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    repo: PathBuf,
    b_repo: PathBuf,
    release: PathBuf,
    _release_dir: tempfile::TempDir,
}

fn project_repo_of(host: &Host) -> PathBuf {
    let project = tddy_projects::project_storage::find_project(
        &host.sessions.path().join("projects"),
        THIS_PROJECT,
    )
    .unwrap()
    .expect("the project is registered");
    PathBuf::from(project.main_repo_path)
}

/// Every checkout in `repo` waits until `release` exists — or until the directory holding it is
/// gone. The world deletes that directory when it is dropped, so a checkout still in flight then (a
/// test that released and returned before the hook's next look, or one that panicked while held)
/// is let go instead of holding the git child, and the test process, forever.
fn hold_checkouts_until(repo: &Path, release: &Path) {
    let hook = repo.join(".git/hooks/post-checkout");
    let release_dir = release.parent().expect("the release file has a directory");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nwhile [ -d '{}' ] && [ ! -f '{}' ]; do sleep 0.1; done\n",
            release_dir.display(),
            release.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

async fn a_world_whose_checkouts_are_held() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let repo = project_repo_of(&a);
    let b_repo = project_repo_of(&b);
    let release_dir = tempfile::tempdir().unwrap();
    let release = release_dir.path().join("release");
    hold_checkouts_until(&repo, &release);
    hold_checkouts_until(&b_repo, &release);
    World {
        a,
        b,
        home,
        repo,
        b_repo,
        release,
        _release_dir: release_dir,
    }
}

impl World {
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }

    fn release_checkouts(&self) {
        std::fs::write(&self.release, "").unwrap();
    }

    /// Start a workspace on `branch`, on `host` (`""` for the home session's own host).
    async fn start_a_workspace_on(&self, branch: &str, host: &str) -> String {
        let response = tokio::time::timeout(
            Duration::from_secs(20),
            self.home_calls(
                "StartSession",
                serde_json::json!({
                    "session_type": "workspace",
                    "daemon_instance_id": host,
                    "branch_worktree_intent": "new_branch_from_base",
                    "new_branch_name": branch,
                }),
            ),
        )
        .await
        .expect("StartSession returns while the worktree step is still held");
        let handle = result_of(&response);
        assert!(
            handle["task_id"].as_str().is_some_and(|t| !t.is_empty()),
            "{handle}"
        );
        let running_on = if host.is_empty() { HOST_A } else { host };
        assert_eq!(handle["daemon_instance_id"], running_on);
        handle["start_id"].as_str().expect("a start_id").to_string()
    }

    async fn start_record(&self, start_id: &str) -> serde_json::Value {
        result_of(
            &self
                .home_calls(
                    "GetSessionStart",
                    serde_json::json!({ "start_id": start_id }),
                )
                .await,
        )
    }

    async fn eventually(
        &self,
        start_id: &str,
        until: impl Fn(&serde_json::Value) -> bool,
    ) -> serde_json::Value {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let record = self.start_record(start_id).await;
            if until(&record) {
                return record;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the start never reached the expected state; last: {record}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn session_ids(&self) -> Vec<String> {
        session_ids_on(&self.a)
    }
}

/// A test that ends on any path — a failed assertion included — lets every held checkout go.
impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.release, "");
    }
}

fn session_ids_on(host: &Host) -> Vec<String> {
    tddy_session_activity::session_reader::list_sessions_in_dir(
        host.sessions.path(),
        &tddy_daemon_kernel::live_sessions::LiveSessions::none(),
    )
    .unwrap()
    .into_iter()
    .map(|entry| entry.session_id)
    .collect()
}

fn result_of(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON result")
}

#[tokio::test]
async fn start_session_returns_a_handle_while_the_worktree_is_still_being_cut() {
    // Given checkouts that are held
    let world = a_world_whose_checkouts_are_held().await;

    // When the home session starts a workspace
    let start_id = world.start_a_workspace_on("async/held", "").await;

    // Then the start reads running, inside its worktree step
    let running = world
        .eventually(&start_id, |r| r["step"]["step"] == "worktree")
        .await;
    assert_eq!(running["state"], "running");
    assert_eq!(running["step"]["status"], "running");
    world.release_checkouts();
}

#[tokio::test]
async fn once_released_the_start_reads_succeeded_with_its_session_branch_and_worktree() {
    // Given a start held in its worktree step
    let world = a_world_whose_checkouts_are_held().await;
    let start_id = world.start_a_workspace_on("async/released", "").await;

    // When the checkout is released
    world.release_checkouts();

    // Then the start succeeds, naming the session, its branch and its worktree
    let done = world
        .eventually(&start_id, |r| r["state"] != "running")
        .await;
    assert_eq!(done["state"], "succeeded", "{done}");
    let session_id = done["session_id"]
        .as_str()
        .expect("the session")
        .to_string();
    assert!(world.session_ids().contains(&session_id));
    assert_eq!(done["branch"], "async/released");
    let worktree = PathBuf::from(done["worktree"].as_str().expect("the worktree"));
    assert_eq!(
        git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "async/released"
    );
}

#[tokio::test]
async fn cancelling_a_held_start_leaves_no_session_and_no_branch() {
    // Given a start held in its worktree step
    let world = a_world_whose_checkouts_are_held().await;
    let sessions_before = world.session_ids();
    let start_id = world.start_a_workspace_on("async/cancelled", "").await;
    world
        .eventually(&start_id, |r| r["step"]["step"] == "worktree")
        .await;

    // When the home session cancels it (and the held checkout is let go, as a cancel waits it out)
    let cancel = world
        .home_calls(
            "CancelSessionStart",
            serde_json::json!({ "start_id": start_id }),
        )
        .await;
    assert!(!cancel.is_error, "{}", cancel.error_message);
    world.release_checkouts();

    // Then the start reads cancelled, and neither a session nor its branch is left behind
    let done = world
        .eventually(&start_id, |r| r["state"] != "running")
        .await;
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(world.session_ids(), sessions_before);
    assert_eq!(
        git_stdout(&world.repo, &["branch", "--list", "async/cancelled"]),
        ""
    );
}

#[tokio::test]
async fn a_start_the_start_path_refuses_is_the_tools_error_naming_why() {
    // Given
    let world = a_world_whose_checkouts_are_held().await;
    world.release_checkouts();

    // When the home session asks for a claude-cli session without a model
    let refused = world
        .home_calls(
            "StartSession",
            serde_json::json!({ "session_type": "claude-cli" }),
        )
        .await;

    // Then it is refused at once, naming the model, with no start to poll
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("model"),
        "{}",
        refused.error_message
    );
}

#[tokio::test]
async fn an_unknown_start_is_refused_naming_it() {
    // Given
    let world = a_world_whose_checkouts_are_held().await;
    world.release_checkouts();

    // When
    let refused = world
        .home_calls(
            "GetSessionStart",
            serde_json::json!({ "start_id": "no-such-start" }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("no-such-start"),
        "{}",
        refused.error_message
    );
}

// ── a start on another host ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_start_on_another_host_is_followed_from_the_orchestrator_until_it_succeeds() {
    // Given checkouts held on host-b too
    let world = a_world_whose_checkouts_are_held().await;

    // When the home session starts a workspace on host-b
    let start_id = world.start_a_workspace_on("async/on-b", HOST_B).await;

    // Then it is followed through host-b's worktree step to its session there
    world
        .eventually(&start_id, |r| r["step"]["step"] == "worktree")
        .await;
    world.release_checkouts();
    let done = world
        .eventually(&start_id, |r| r["state"] != "running")
        .await;
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["daemon_instance_id"], HOST_B);
    let session_id = done["session_id"]
        .as_str()
        .expect("the session")
        .to_string();
    assert!(session_ids_on(&world.b).contains(&session_id));
    assert!(!world.session_ids().contains(&session_id));
}

#[tokio::test]
async fn cancelling_a_start_on_another_host_stops_it_there() {
    // Given a start held in host-b's worktree step
    let world = a_world_whose_checkouts_are_held().await;
    let sessions_on_b_before = session_ids_on(&world.b);
    let start_id = world
        .start_a_workspace_on("async/cancelled-on-b", HOST_B)
        .await;
    world
        .eventually(&start_id, |r| r["step"]["step"] == "worktree")
        .await;

    // When the home session cancels it
    let cancel = world
        .home_calls(
            "CancelSessionStart",
            serde_json::json!({ "start_id": start_id }),
        )
        .await;
    assert!(!cancel.is_error, "{}", cancel.error_message);
    world.release_checkouts();

    // Then host-b's start is stopped: no session and no branch there
    let done = world
        .eventually(&start_id, |r| r["state"] != "running")
        .await;
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(session_ids_on(&world.b), sessions_on_b_before);
    assert_eq!(
        git_stdout(&world.b_repo, &["branch", "--list", "async/cancelled-on-b"]),
        ""
    );
}

// ── what a start on a target did there (`#coder-start` 8/10) ─────────────────────────────────

/// The ssh alias the operator's config lists, and the derived host id the tool names it by.
const TARGET_ALIAS: &str = "remote-dev";
const TARGET_HOST: &str = "host-a/ssh/remote-dev";
/// A credential a project's git URL carries, which no log may show.
const SECRET: &str = "s3cret-token";

/// A machine behind a stand-in `ssh` first on this process's `PATH`: it runs the remote command
/// here, with the machine's `/home/dev` mapped under its own directory — except `git fetch` and
/// `git clone`, which it fails as a target that cannot reach its remote does, the way a real one
/// echoes the URL it could not reach.
struct AMachineWhoseRemoteIsUnreachable {
    /// The stand-in `claude` this host resolves its agent to: a start on a target resolves the
    /// agent here before it runs the target's own, and the start fails before either runs.
    host_claude: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
}

impl AMachineWhoseRemoteIsUnreachable {
    fn behind_ssh() -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the machine");
        let root = std::fs::canonicalize(dir.path()).expect("the directory resolves");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let ssh = bin.join("ssh");
        std::fs::write(
            &ssh,
            format!(
                r#"#!/bin/sh
for command; do :; done
case "$command" in
  *"fetch --all"*)
    echo "fatal: could not read from remote repository." >&2
    exit 128;;
  *"git clone"*)
    echo "fatal: unable to access 'https://bot:{SECRET}@example.com/acme/repo.git/': Could not resolve host" >&2
    exit 128;;
esac
command=$(printf '%s' "$command" | /usr/bin/sed 's#/home/dev#{root}/home/dev#g')
export HOME='{root}'
exec /bin/sh -c "$command" </dev/null
"#,
                root = root.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A freshly written executable pays a first-execution scan; pay it before any timed call.
        let _ = std::process::Command::new(&ssh).arg("warm-up").output();
        let host_claude = root.join("host-bin/claude");
        std::fs::create_dir_all(host_claude.parent().unwrap()).unwrap();
        std::fs::write(&host_claude, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&host_claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let previous_path = std::env::var_os("PATH");
        let mut path = std::ffi::OsString::from(&bin);
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            host_claude,
            previous_path,
            _dir: dir,
        }
    }
}

impl Drop for AMachineWhoseRemoteIsUnreachable {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// A home session on a host whose operator lists [`TARGET_ALIAS`], the machine behind it, and
/// the project's git URL as given.
struct ATargetWorld {
    a: Host,
    home: String,
    _machine: AMachineWhoseRemoteIsUnreachable,
    _operator_home: tempfile::TempDir,
}

async fn a_world_with_a_target(git_url: &str) -> ATargetWorld {
    let machine = AMachineWhoseRemoteIsUnreachable::behind_ssh();
    let peers = Arc::new(InProcessPeers::default());
    let (a, operator_home) = support::a_host_as_current_user_with_operator_home_running_claude_as(
        HOST_A,
        &peers,
        &machine.host_claude,
    );
    std::fs::create_dir_all(operator_home.path().join(".ssh")).unwrap();
    std::fs::write(
        operator_home.path().join(".ssh/config"),
        format!("Host {TARGET_ALIAS}\n    HostName 10.0.0.7\n"),
    )
    .unwrap();
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let projects_dir = a.sessions.path().join("projects");
    let mut project = tddy_projects::project_storage::find_project(&projects_dir, THIS_PROJECT)
        .unwrap()
        .expect("the project is registered");
    project.git_url = git_url.to_string();
    tddy_projects::project_storage::write_projects(&projects_dir, &[project]).unwrap();
    ATargetWorld {
        a,
        home,
        _machine: machine,
        _operator_home: operator_home,
    }
}

impl ATargetWorld {
    async fn calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }

    /// Start a claude-cli session on the target, and read its record once it is no longer running.
    async fn start_on_the_target_and_wait(&self) -> serde_json::Value {
        let handle = result_of(
            &self
                .calls(
                    "StartSession",
                    serde_json::json!({
                        "session_type": "claude-cli",
                        "model": "claude-opus-4-8",
                        "target": TARGET_HOST,
                        "branch_worktree_intent": "new_branch_from_base",
                        "new_branch_name": "remote/failing",
                    }),
                )
                .await,
        );
        let start_id = handle["start_id"].as_str().expect("a start_id").to_string();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let record = result_of(
                &self
                    .calls(
                        "GetSessionStart",
                        serde_json::json!({ "start_id": start_id }),
                    )
                    .await,
            );
            if record["state"] != "running" {
                return record;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the start never ended; last: {record}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_start_whose_fetch_fails_reports_the_failing_call_and_names_its_step() {
    // Given a target whose remote cannot be fetched from
    let world = a_world_with_a_target("").await;

    // When the home session starts a claude-cli session on it
    let record = world.start_on_the_target_and_wait().await;

    // Then the start failed, its error naming the step that failed
    assert_eq!(record["state"], "failed", "{record}");
    let error = record["error"].as_str().expect("why it failed");
    assert!(error.contains("fetching on the target failed"), "{error}");
    // And its log ends with the fetch: the caller's command, exit 128, the stderr tail, the host
    let calls = record["remote_calls"]
        .as_array()
        .expect("a remote_calls list");
    let fetch = calls.last().expect("the start made remote calls");
    assert_eq!(fetch["exit_code"], 128, "{record}");
    assert_eq!(fetch["outcome"], "exited", "{record}");
    assert!(
        fetch["command"].as_str().unwrap().contains("fetch --all"),
        "{fetch}"
    );
    assert!(
        fetch["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("could not read from remote repository"),
        "{fetch}"
    );
    assert_eq!(fetch["host"], "ssh target remote-dev", "{fetch}");
    // And the calls before it are listed too, the home probe first, none wrapped by a transport
    assert!(calls.len() >= 3, "{record}");
    assert!(
        calls[0]["command"].as_str().unwrap().contains("$HOME"),
        "the first call is the home probe: {record}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn no_credential_reaches_the_remote_call_log() {
    // Given a project whose git URL carries a password, and a target that fails the clone with it
    let world =
        a_world_with_a_target(&format!("https://bot:{SECRET}@example.com/acme/repo.git")).await;

    // When the home session starts a claude-cli session on the target
    let record = world.start_on_the_target_and_wait().await;

    // Then the clone is in the log, failed, and the password is in none of it
    let calls = record["remote_calls"]
        .as_array()
        .expect("a remote_calls list");
    assert!(
        calls
            .iter()
            .any(|call| call["command"].as_str().unwrap().contains("git clone")),
        "the clone call is logged: {record}"
    );
    let log = serde_json::to_string(calls).unwrap();
    assert!(!log.contains(SECRET), "{log}");
}
