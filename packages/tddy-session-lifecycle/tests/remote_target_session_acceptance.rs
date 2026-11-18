//! Acceptance: a session whose agent runs on a **target** — a machine the daemon reaches through a
//! remote shell, with no tddy software on it — through the real `DaemonSessionHost`.
//!
//! The target is a stand-in `ssh` executable first on this process's `PATH`: a script that stands
//! for both the OpenSSH client and the host it reaches. It runs the remote command on this machine
//! as a child of its own, with the target's `/home/dev` mapped onto a temp directory, so the
//! worktree the daemon creates "there", the files it writes "there" and the agent it starts
//! "there" are all observable here, and nothing opens a connection. The agent is `claude.yaml`
//! dropped in under another id, its binary a stub that records each run.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-ssh-agent-launch.md.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    session_target, DeleteSessionRequest, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, SessionTarget, SignalSessionRequest, SshTarget,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_task::TaskStatus;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

mod agent_registry_support;
mod cli_harness_support;
use agent_registry_support::{builtin_yaml_as, AJailGuard};
use cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, register_project, session_host, VALID_TOKEN,
};

const TEST_PROJECT_ID: &str = "remote-target-project";
/// The agent: `claude.yaml` under this id, so its sessions launch through the generic CLI path.
const AGENT_ID: &str = "acme";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
/// The branch a start names: `claude.yaml` declares `branch_name_required`.
const BRANCH: &str = "acme-work";
/// The ssh `Host` alias the target is known by.
const TARGET_ALIAS: &str = "buildbox";
/// The status the stub agent exits with when it is told to (`SIGUSR1`).
const AGENT_EXIT_STATUS: i32 = 7;

/// A safety net for an agent to be observed on the target, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(20);

/// The login shell of the target: `$SHELL -lc <command>` runs the command under the `PATH` the
/// stand-in set, whose first directory is the target's own `opt/agents` — where its agent is
/// installed, as a target start runs the agent's bare command, never this host's path to it.
const A_TARGET_LOGIN_SHELL: &str =
    "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\nexec /bin/sh -c \"$2\"\n";

fn write_executable(path: &Path, script: &str) {
    // A freshly written executable pays a first-execution scan on this OS (~2.4 s measured on
    // macOS, against ~20 ms afterwards): a stand-in or stub written here and then timed from
    // its first real exec would spend that scan inside whatever window the test measures. So
    // the script carries a warm-up guard as its first line, and is run once here, guarded —
    // no runs are recorded, no pid files written — to pay the scan before any launch is timed.
    let script = match script.strip_prefix("#!/bin/sh\n") {
        Some(body) => {
            format!("#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n{body}")
        }
        None => script.to_string(),
    };
    std::fs::write(path, script).expect("the script is written");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("the script is made executable");
    let warmed = std::process::Command::new(path)
        .env("TDDY_FIXTURE_WARMUP", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the freshly written script warms up");
    assert!(
        warmed.status.success(),
        "the warm-up of {} exited {}: {}",
        path.display(),
        warmed.status,
        String::from_utf8_lossy(&warmed.stderr)
    );
}

/// The target a session's agent runs on, for as long as it lives: a stand-in `ssh` first on this
/// process's `PATH`, and the directory standing for the target's filesystem.
struct ATarget {
    root: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
}

impl ATarget {
    /// A target that runs what it is asked to. The stand-in runs its last argument — the remote
    /// command — with `sh -c` as a child whose terminal is its own, every `/home/dev` path in it
    /// mapped under the target's directory. It hangs the command up when it is itself hung up or
    /// terminated, and exits with the command's status.
    fn reachable() -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the target");
        let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("a directory for the stand-in ssh");
        write_executable(&bin.join("login-shell"), A_TARGET_LOGIN_SHELL);
        std::fs::create_dir_all(root.join("opt/agents")).expect("the target's agents directory");
        write_executable(
            &bin.join("ssh"),
            &format!(
                r#"#!/bin/sh
for command; do :; done
command=$(printf '%s' "$command" | /usr/bin/sed 's#/home/dev#{root}/home/dev#g')
export HOME='{root}'
SHELL='{root}/bin/login-shell'
PATH='{root}/opt/agents':"$PATH"
export SHELL PATH
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
exit $?
"#,
                root = root.display()
            ),
        );
        let previous_path = std::env::var_os("PATH");
        let mut path = std::ffi::OsString::from(&bin);
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            root,
            previous_path,
            _dir: dir,
        }
    }

    /// Where the target keeps the worktree of `session_id`: under the checkout resolved from
    /// the target user's home — the stub's `$HOME` is this directory, and the project's empty
    /// git URL names `repo` under `.tddy/repos`.
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        self.root
            .join(".tddy/repos/repo/.worktrees")
            .join(session_id)
    }

    /// Where the pointer of `session_id` lives on the target.
    fn pointer_of(&self, session_id: &str) -> PathBuf {
        self.worktree_of(session_id).join(".tddy/session.json")
    }
}

impl Drop for ATarget {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// One run of the stub agent, as it recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AnAgentRun {
    pid: i32,
    cwd: PathBuf,
    args: Vec<String>,
}

impl AnAgentRun {
    fn is_a_resume(&self) -> bool {
        self.args.iter().any(|arg| arg == "--resume")
    }

    fn is_alive(&self) -> bool {
        // SAFETY: signal 0 only probes whether the pid still exists.
        unsafe { libc::kill(self.pid, 0) == 0 }
    }
}

/// A stub agent: appends one line per run — its pid, its working directory and its arguments — to
/// `runs`, prints a banner, then stays up until it is hung up, or exits [`AGENT_EXIT_STATUS`] on
/// `SIGUSR1`.
fn a_stub_agent(runs: &Path) -> String {
    format!(
        r#"#!/bin/sh
{{ printf '%s\t%s' "$$" "$(/bin/pwd -P)"; for arg in "$@"; do printf '\t%s' "$arg"; done; printf '\n'; }} >> '{runs}'
trap 'exit {AGENT_EXIT_STATUS}' USR1
printf 'ACME UP\n'
while :; do /bin/sleep 0.1; done
"#,
        runs = runs.display()
    )
}

/// A daemon with one registered project, the `acme` agent installed, and a target its sessions'
/// agents can run on.
struct ADaemonWithATarget {
    service: Arc<DaemonSessionHost>,
    manager: Arc<CliSessionManager>,
    sessions_base: PathBuf,
    runs: PathBuf,
    target: ATarget,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_daemon_with_a_reachable_target() -> ADaemonWithATarget {
    let target = ATarget::reachable();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = std::fs::canonicalize(sessions_dir.path()).unwrap();
    register_project(
        &sessions_base.join("projects"),
        repo_dir.path(),
        TEST_PROJECT_ID,
    );
    let tools = sessions_base.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("acme.yaml"), builtin_yaml_as("claude", AGENT_ID)).unwrap();

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let runs = stub_bin.join("runs");
    let stub = stub_bin.join(AGENT_ID);
    write_executable(&stub, &a_stub_agent(&runs));
    // A target start runs the agent's bare command, found on the target's own `PATH`.
    write_executable(
        &target.root.join("opt/agents").join(AGENT_ID),
        &a_stub_agent(&runs),
    );

    let manager = Arc::new(CliSessionManager::new());
    let service = Arc::new(session_host(
        a_daemon_config(),
        SESSION_TYPE,
        &stub,
        sessions_base.clone(),
        Arc::clone(&manager),
    ));
    // As the daemon runtime does: a jailed start reaches its host through this bridge.
    service.install_sandbox_rpc_bridge();
    ADaemonWithATarget {
        service,
        manager,
        sessions_base,
        runs,
        target,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

/// The target a start names: the ssh host [`TARGET_ALIAS`].
fn the_ssh_target() -> SessionTarget {
    SessionTarget {
        kind: Some(session_target::Kind::Ssh(SshTarget {
            config_host: TARGET_ALIAS.to_string(),
        })),
    }
}

/// A start of an `acme-cli` session whose agent runs on [`the_ssh_target`].
fn a_start_on_the_target() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: SESSION_TYPE.to_string(),
        model: MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: BRANCH.to_string(),
        target: Some(the_ssh_target()),
        ..Default::default()
    }
}

impl ADaemonWithATarget {
    async fn start(&self, req: StartSessionRequest) -> Result<String, tddy_rpc::Status> {
        self.service
            .start_session(Request::direct(req))
            .await
            .map(|started| started.into_inner().session_id)
    }

    /// A started session whose agent runs on the target.
    async fn a_session_on_the_target(&self) -> String {
        self.start(a_start_on_the_target())
            .await
            .expect("a start on a reachable target succeeds")
    }

    /// What a refused start was refused with: its code and its message. A start that was not
    /// refused answers its session, whose agent — jailed or not — is terminated.
    async fn refusal_of(&self, req: StartSessionRequest) -> Result<String, (Code, String)> {
        self.start(req)
            .await
            .inspect(|session_id| {
                let _started_anyway = AJailGuard(
                    read_session_metadata(&self.session_dir(session_id))
                        .ok()
                        .and_then(|metadata| metadata.pid),
                );
            })
            .map_err(|status| (status.code(), status.message().to_string()))
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// Every run of the stub agent so far, oldest first.
    fn agent_runs(&self) -> Vec<AnAgentRun> {
        std::fs::read_to_string(&self.runs)
            .unwrap_or_default()
            .lines()
            .map(|line| line.split('\t').map(str::to_string).collect::<Vec<_>>())
            .filter(|fields| fields.len() >= 2)
            .map(|fields| AnAgentRun {
                pid: fields[0].parse().expect("a run records its pid"),
                cwd: PathBuf::from(&fields[1]),
                args: fields[2..].to_vec(),
            })
            .collect()
    }

    /// The runs of `session_id`'s agent on the target — in its worktree there — once there are
    /// `count` of them.
    async fn runs_on_the_target(&self, session_id: &str, count: usize) -> Vec<AnAgentRun> {
        let worktree = self.target.worktree_of(session_id);
        eventually(
            &format!("{count} run(s) of the agent in {}", worktree.display()),
            OBSERVE,
            || {
                let every_run = self.agent_runs();
                let on_the_target: Vec<AnAgentRun> = every_run
                    .iter()
                    .filter(|run| run.cwd == worktree)
                    .cloned()
                    .collect();
                (on_the_target.len() >= count)
                    .then_some(on_the_target)
                    .ok_or_else(|| format!("the agent's runs so far: {every_run:?}"))
            },
        )
        .await
    }

    /// `session_id`'s agent, running on the target.
    async fn the_agent_on_the_target(&self, session_id: &str) -> AnAgentRun {
        self.runs_on_the_target(session_id, 1)
            .await
            .pop()
            .expect("one run was waited for")
    }

    /// Stop `session_id` as an operator does: signal it, and wait for its terminal to end.
    async fn stop(&self, session_id: &str) {
        self.service
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                signal: tddy_service::proto::session::Signal::Sigterm as i32,
                control_token: String::new(),
            }))
            .await
            .expect("the session is signalled");
        self.the_terminal_has_ended(session_id).await;
    }

    /// Wait until `session_id` has no main terminal, its exit hook settled.
    async fn the_terminal_has_ended(&self, session_id: &str) {
        eventually_awaiting(
            &format!("session {session_id}'s terminal to end"),
            OBSERVE,
            || async {
                match self.manager.get(session_id).await {
                    None => Ok(()),
                    Some(_) => Err("the terminal is still registered".to_string()),
                }
            },
        )
        .await;
        self.manager.exit_hook_settled(session_id).await;
    }

    async fn resume(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn delete(&self, session_id: &str) -> Result<(), tddy_rpc::Status> {
        self.service
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    /// The target `session_id` is listed with.
    async fn listed_target_of(&self, session_id: &str) -> Option<SessionTarget> {
        self.service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("the sessions are listed")
            .into_inner()
            .sessions
            .into_iter()
            .find(|entry| entry.session_id == session_id)
            .expect("the session is listed")
            .target
    }
}

/// Wait until `agent`'s process is gone.
async fn the_agent_has_ended(agent: &AnAgentRun) {
    eventually("the agent on the target to end", OBSERVE, || {
        (!agent.is_alive())
            .then_some(())
            .ok_or_else(|| format!("pid {} is still alive", agent.pid))
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_started_target_session_records_its_target() {
    // Given
    let daemon = a_daemon_with_a_reachable_target();

    // When
    let session_id = daemon.a_session_on_the_target().await;

    // Then the record names the target and its worktree there — and no ssh exec host, which is the
    // other mode
    let metadata =
        read_session_metadata(&daemon.session_dir(&session_id)).expect(".session.yaml is written");
    let worktree_on_the_target = daemon
        .target
        .worktree_of(&session_id)
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        (
            metadata.target.clone(),
            metadata.ssh_config_host.clone(),
            metadata.repo_path.clone(),
        ),
        (
            Some(tddy_core::SessionTarget::ssh(TARGET_ALIAS)),
            None,
            Some(worktree_on_the_target),
        )
    );

    // And the listing carries it, for a session that reads no path of this host
    assert_eq!(
        (
            daemon.listed_target_of(&session_id).await,
            tddy_core::reads_host_paths(&metadata),
        ),
        (Some(the_ssh_target()), false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn stopping_the_session_ends_the_remote_agent() {
    // Given a session whose agent runs on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    let agent = daemon.the_agent_on_the_target(&session_id).await;

    // When
    daemon.stop(&session_id).await;

    // Then the agent on the target is gone with the session's terminal
    the_agent_has_ended(&agent).await;
    assert_eq!(
        (
            agent.is_alive(),
            daemon.manager.get(&session_id).await.is_some()
        ),
        (false, false)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_remote_agents_exit_status_ends_the_terminal() {
    // Given a session whose agent runs on the target, and its main terminal
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    let agent = daemon.the_agent_on_the_target(&session_id).await;
    let terminal = daemon
        .manager
        .get(&session_id)
        .await
        .expect("the session has a main terminal");

    // When the agent exits on the target, of its own accord
    // SAFETY: a plain kill(2) of the stub this test started.
    unsafe { libc::kill(agent.pid, libc::SIGUSR1) };
    daemon.the_terminal_has_ended(&session_id).await;

    // Then the terminal ended with the agent's exit status
    assert_eq!(
        terminal.task_status(),
        TaskStatus::Completed {
            exit_code: Some(AGENT_EXIT_STATUS)
        }
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_resume_starts_the_agent_on_the_target_again() {
    // Given a session on the target whose agent was stopped
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    let first = daemon.the_agent_on_the_target(&session_id).await;
    daemon.stop(&session_id).await;
    the_agent_has_ended(&first).await;

    // When
    daemon
        .resume(&session_id)
        .await
        .expect("a stopped session on a target resumes");
    let runs = daemon.runs_on_the_target(&session_id, 2).await;

    // Then the agent ran in the target's worktree twice: started, then resumed
    assert_eq!(
        runs.iter().map(AnAgentRun::is_a_resume).collect::<Vec<_>>(),
        vec![false, true]
    );

    // And the resumed agent is up, the session's terminal with it
    assert_eq!(
        (
            runs[1].is_alive(),
            daemon.manager.get(&session_id).await.is_some()
        ),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_with_managed_codebase_is_refused() {
    // Given
    let daemon = a_daemon_with_a_reachable_target();

    // When a start names a target and a managed codebase
    let refused = daemon
        .refusal_of(StartSessionRequest {
            managed_codebase: true,
            ..a_start_on_the_target()
        })
        .await;

    // Then it is refused, naming both
    assert_eq!(
        refused.map_err(|(code, message)| (
            code,
            message.starts_with("target cannot be combined with managed_codebase: ")
        )),
        Err((Code::InvalidArgument, true))
    );
    assert_eq!(daemon.agent_runs(), Vec::new());
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_with_a_sandbox_is_refused() {
    // Given
    let daemon = a_daemon_with_a_reachable_target();

    // When a start names a target and a sandbox
    let refused = daemon
        .refusal_of(StartSessionRequest {
            sandbox: true,
            ..a_start_on_the_target()
        })
        .await;

    // Then it is refused, naming both
    assert_eq!(
        refused.map_err(|(code, message)| (
            code,
            message.starts_with("target cannot be combined with sandbox: ")
        )),
        Err((Code::InvalidArgument, true))
    );
    assert_eq!(daemon.agent_runs(), Vec::new());
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_with_a_split_placement_is_refused() {
    // Given
    let daemon = a_daemon_with_a_reachable_target();

    // When a start names a target and another daemon to hold its worktree
    let refused = daemon
        .refusal_of(StartSessionRequest {
            codebase_daemon_instance_id: "another-daemon".to_string(),
            ..a_start_on_the_target()
        })
        .await;

    // Then it is refused, naming both
    assert_eq!(
        refused.map_err(|(code, message)| (
            code,
            message.starts_with("target cannot be combined with codebase_daemon_instance_id")
        )),
        Err((Code::InvalidArgument, true))
    );
    assert_eq!(daemon.agent_runs(), Vec::new());
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_and_an_ssh_config_host_together_are_refused() {
    // Given
    let daemon = a_daemon_with_a_reachable_target();

    // When a start names a target for its agent and an ssh host for its tools
    let refused = daemon
        .refusal_of(StartSessionRequest {
            ssh_config_host: TARGET_ALIAS.to_string(),
            ..a_start_on_the_target()
        })
        .await;

    // Then it is refused, naming both
    assert_eq!(
        refused.map_err(|(code, message)| (
            code,
            message.starts_with("target cannot be combined with ssh_config_host: ")
        )),
        Err((Code::InvalidArgument, true))
    );
    assert_eq!(daemon.agent_runs(), Vec::new());
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_target_session_removes_its_worktree_and_pointer_on_the_target() {
    // Given a stopped session whose worktree and pointer are on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    let agent = daemon.the_agent_on_the_target(&session_id).await;
    let worktree = daemon.target.worktree_of(&session_id);
    let pointer = daemon.target.pointer_of(&session_id);
    assert_eq!(
        (worktree.is_dir(), pointer.is_file()),
        (true, true),
        "the target holds the session's worktree and pointer while it runs"
    );
    daemon.stop(&session_id).await;
    the_agent_has_ended(&agent).await;

    // When
    daemon
        .delete(&session_id)
        .await
        .expect("a stopped session on a target is deleted");

    // Then nothing of the session is left on the target, nor on this host
    assert_eq!(
        (
            worktree.exists(),
            pointer.exists(),
            daemon.session_dir(&session_id).exists(),
        ),
        (false, false, false)
    );
}
