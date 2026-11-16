//! Acceptance: a session whose agent runs in a **Coder workspace** — its target names one of the
//! caller's Coder accounts and a workspace of it — through the real `DaemonSessionHost`.
//!
//! The workspace is a stand-in `coder` executable first on this process's `PATH`: a script that
//! stands for both the Coder CLI and the workspace it reaches. It runs what follows `--` on this
//! machine as a child of its own, with the workspace's `/home/dev` mapped onto a temp directory,
//! as the stand-in `ssh` of `remote_target_session_acceptance.rs` does; a stopped workspace is one
//! whose stand-in refuses before it runs anything. The caller's vault is a stand-in that can be
//! locked between a start and a resume. No deployment is reached.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_credentials::SecretString;
use tddy_host_service::coder_derived_hosts::CoderAccountRef;
use tddy_host_service::coder_workspaces::{CoderCredentialSource, CoderCredentials};
use tddy_host_service::test_util::{a_coder_vault_of, ACoderVault};
use tddy_rpc::{Code, Request};
use tddy_service::proto::session::{
    session_target, CoderTarget, ListSessionsRequest, ResumeSessionRequest,
    SessionService as SessionServiceTrait, SessionTarget, Signal, SignalSessionRequest,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

mod agent_registry_support;
mod cli_harness_support;
use agent_registry_support::{builtin_yaml_as, AJailGuard};
use cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, current_os_user, register_project, session_host,
    VALID_TOKEN,
};

const TEST_PROJECT_ID: &str = "coder-target-project";
/// The agent: `claude.yaml` under this id, so its sessions launch through the generic CLI path.
const AGENT_ID: &str = "acme";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
/// The branch a start names: `claude.yaml` declares `branch_name_required`.
const BRANCH: &str = "acme-work";

/// The caller's Coder account, as the accounts listing names it.
const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";
const ADAS_DEPLOYMENT: &str = "https://coder.example.com";
const WORKSPACE_OWNER: &str = "ada";
const WORKSPACE: &str = "devbox";
/// What the stand-in `coder` says for a workspace that is not running.
const STOPPED_COMPLAINT: &str = "workspace ada/devbox is stopped";

/// A safety net for an agent to be observed in the workspace, not a prediction of how long it
/// takes.
const OBSERVE: Duration = Duration::from_secs(20);

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

/// The workspace a session's agent runs in, for as long as it lives: a stand-in `coder` first on
/// this process's `PATH`, and the directory standing for the workspace's filesystem.
struct AWorkspace {
    root: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
}

impl AWorkspace {
    /// A running workspace. The stand-in hands what follows `--` to `sh -c` as one command line —
    /// which is what a workspace's login shell is given — as a child whose terminal is its own,
    /// every `/home/dev` path in it mapped under the workspace's directory. It hangs the command
    /// up when it is itself hung up or terminated, and exits with the command's status.
    fn running() -> Self {
        Self::reached_through(|root| {
            format!(
                r#"while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do shift; done
shift
command=$(printf '%s' "$*" | /usr/bin/sed 's#/home/dev#{root}/home/dev#g')
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
exit $?
"#,
                root = root.display()
            )
        })
    }

    /// A stopped workspace: the stand-in refuses before anything runs, as the CLI does for a
    /// workspace it is told not to start — [`STOPPED_COMPLAINT`] on stderr and exit status 1.
    fn stopped() -> Self {
        Self::reached_through(|_root| format!("echo '{STOPPED_COMPLAINT}' >&2\nexit 1\n"))
    }

    fn reached_through(coder_body: impl FnOnce(&Path) -> String) -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the workspace");
        let root = std::fs::canonicalize(dir.path()).expect("the workspace's directory resolves");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("a directory for the stand-in coder");
        write_executable(
            &bin.join("coder"),
            &format!("#!/bin/sh\n{}", coder_body(&root)),
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

    /// Where the workspace keeps the worktree of `session_id`: under the checkout the daemon
    /// clones a project into on a target (`tddy_core::ssh_exec::default_remote_repo_root`).
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        let checkout = tddy_core::ssh_exec::default_remote_repo_root("");
        self.root
            .join(checkout.trim_start_matches('/'))
            .join(".worktrees")
            .join(session_id)
    }
}

impl Drop for AWorkspace {
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
/// `runs`, prints a banner, then stays up until it is hung up.
fn a_stub_agent(runs: &Path) -> String {
    format!(
        r#"#!/bin/sh
{{ printf '%s\t%s' "$$" "$(/bin/pwd -P)"; for arg in "$@"; do printf '\t%s' "$arg"; done; printf '\n'; }} >> '{runs}'
printf 'ACME UP\n'
while :; do /bin/sleep 0.1; done
"#,
        runs = runs.display()
    )
}

/// A daemon with one registered project, the `acme` agent installed, the caller's vault holding a
/// Coder account, and a workspace of that account its sessions' agents can run in.
struct ADaemonWithAWorkspace {
    service: Arc<DaemonSessionHost>,
    manager: Arc<CliSessionManager>,
    vault: Arc<ACoderVault>,
    sessions_base: PathBuf,
    runs: PathBuf,
    workspace: AWorkspace,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_daemon_reaching_a_running_workspace() -> ADaemonWithAWorkspace {
    a_daemon_reaching(AWorkspace::running())
}

fn a_daemon_reaching_a_stopped_workspace() -> ADaemonWithAWorkspace {
    a_daemon_reaching(AWorkspace::stopped())
}

fn a_daemon_reaching(workspace: AWorkspace) -> ADaemonWithAWorkspace {
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

    // The vault of the person VALID_TOKEN signs in — the subject a Coder target is resolved as.
    let vault = Arc::new(a_coder_vault_of(&current_os_user()).holding(
        CoderAccountRef::new(ADAS_ACCOUNT, "Work"),
        CoderCredentials::new(
            ADAS_DEPLOYMENT,
            SecretString::new("adas-coder-session-token"),
        ),
    ));
    let manager = Arc::new(CliSessionManager::new());
    let service = Arc::new(
        session_host(
            a_daemon_config(),
            SESSION_TYPE,
            &stub,
            sessions_base.clone(),
            Arc::clone(&manager),
        )
        .with_coder_credentials(Arc::clone(&vault) as Arc<dyn CoderCredentialSource>),
    );
    // As the daemon runtime does: a jailed start reaches its host through this bridge.
    service.install_sandbox_rpc_bridge();
    ADaemonWithAWorkspace {
        service,
        manager,
        vault,
        sessions_base,
        runs,
        workspace,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

/// The target a start names: the caller's workspace [`WORKSPACE`] of [`ADAS_ACCOUNT`].
fn the_coder_target() -> SessionTarget {
    SessionTarget {
        kind: Some(session_target::Kind::Coder(CoderTarget {
            account_id: ADAS_ACCOUNT.to_string(),
            owner: WORKSPACE_OWNER.to_string(),
            workspace: WORKSPACE.to_string(),
            agent: String::new(),
        })),
    }
}

/// [`the_coder_target`], as a session's record keeps it.
fn the_recorded_coder_target() -> tddy_core::SessionTarget {
    tddy_core::SessionTarget::coder(ADAS_ACCOUNT, WORKSPACE_OWNER, WORKSPACE)
}

/// A start of an `acme-cli` session whose agent runs in [`the_coder_target`].
fn a_start_in_the_workspace() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: SESSION_TYPE.to_string(),
        model: MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: BRANCH.to_string(),
        target: Some(the_coder_target()),
        ..Default::default()
    }
}

/// What a refused call was refused with: its code and its message.
fn refusal(status: tddy_rpc::Status) -> (Code, String) {
    (status.code(), status.message().to_string())
}

impl ADaemonWithAWorkspace {
    /// Start a session in the workspace. A start that was refused answers its code and message.
    async fn start_in_the_workspace(&self) -> Result<String, (Code, String)> {
        self.service
            .start_session(Request::direct(a_start_in_the_workspace()))
            .await
            .map(|started| started.into_inner().session_id)
            .inspect(|session_id| {
                // Whatever the start launched is terminated with the test, jailed or not.
                let _started = AJailGuard(
                    read_session_metadata(&self.session_dir(session_id))
                        .ok()
                        .and_then(|metadata| metadata.pid),
                );
            })
            .map_err(refusal)
    }

    /// A started session whose agent runs in the workspace.
    async fn a_session_in_the_workspace(&self) -> String {
        self.service
            .start_session(Request::direct(a_start_in_the_workspace()))
            .await
            .expect("a start in a running workspace, with the vault open, succeeds")
            .into_inner()
            .session_id
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

    /// The runs of `session_id`'s agent in the workspace — in its worktree there — once there are
    /// `count` of them.
    async fn runs_in_the_workspace(&self, session_id: &str, count: usize) -> Vec<AnAgentRun> {
        let worktree = self.workspace.worktree_of(session_id);
        eventually(
            &format!("{count} run(s) of the agent in {}", worktree.display()),
            OBSERVE,
            || {
                let every_run = self.agent_runs();
                let in_the_workspace: Vec<AnAgentRun> = every_run
                    .iter()
                    .filter(|run| run.cwd == worktree)
                    .cloned()
                    .collect();
                (in_the_workspace.len() >= count)
                    .then_some(in_the_workspace)
                    .ok_or_else(|| format!("the agent's runs so far: {every_run:?}"))
            },
        )
        .await
    }

    /// Stop `session_id` as an operator does: signal it, and wait for its terminal to end.
    async fn stop(&self, session_id: &str) {
        self.service
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                signal: Signal::Sigterm as i32,
                control_token: String::new(),
            }))
            .await
            .expect("the session is signalled");
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

    async fn resume(&self, session_id: &str) -> Result<(), (Code, String)> {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
            .map_err(refusal)
    }

    /// The ids of the sessions this daemon lists.
    async fn listed_sessions(&self) -> Vec<String> {
        self.service
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
            }))
            .await
            .expect("the sessions are listed")
            .into_inner()
            .sessions
            .into_iter()
            .map(|entry| entry.session_id)
            .collect()
    }

    /// The target `session_id` recorded in its `.session.yaml`.
    fn recorded_target_of(&self, session_id: &str) -> Option<tddy_core::SessionTarget> {
        read_session_metadata(&self.session_dir(session_id))
            .expect(".session.yaml is written")
            .target
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_stopped_workspace_is_refused_as_a_target_with_that_reason() {
    // Given a daemon whose caller's workspace is stopped
    let daemon = a_daemon_reaching_a_stopped_workspace();

    // When a session is started in it
    let refused = daemon.start_in_the_workspace().await;

    // Then the start is refused, saying what the CLI said of the workspace
    assert_eq!(
        refused.map_err(|(_code, message)| message.contains(STOPPED_COMPLAINT)),
        Err(true)
    );

    // And nothing is left of it: the agent never ran anywhere, and no session is listed
    assert_eq!(
        (daemon.agent_runs(), daemon.listed_sessions().await),
        (Vec::new(), Vec::<String>::new())
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_resume_with_an_open_vault_restarts_the_agent_in_the_workspace() {
    // Given a session in the workspace whose agent was stopped, and the caller's vault still open
    let daemon = a_daemon_reaching_a_running_workspace();
    let session_id = daemon.a_session_in_the_workspace().await;
    daemon.runs_in_the_workspace(&session_id, 1).await;
    daemon.stop(&session_id).await;

    // When
    let resumed = daemon.resume(&session_id).await;
    let runs = daemon.runs_in_the_workspace(&session_id, 2).await;

    // Then the agent ran in the workspace's worktree twice: started, then resumed
    assert_eq!(
        (
            resumed,
            runs.iter().map(AnAgentRun::is_a_resume).collect::<Vec<_>>()
        ),
        (Ok(()), vec![false, true])
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
async fn a_resume_with_a_locked_vault_is_refused_and_the_session_is_kept() {
    // Given a stopped session in the workspace, and a caller who has since locked their vault
    let daemon = a_daemon_reaching_a_running_workspace();
    let session_id = daemon.a_session_in_the_workspace().await;
    daemon.stop(&session_id).await;
    let runs_before = daemon.agent_runs().len();
    daemon.vault.lock();

    // When
    let refused = daemon.resume(&session_id).await;

    // Then the resume is refused because the vault is locked, saying how to go on
    assert_eq!(
        refused,
        Err((
            Code::FailedPrecondition,
            "the vault holding this session's Coder account is locked: unlock it and try again"
                .to_string()
        ))
    );

    // And the session is kept as it was — listed, still naming its workspace, its agent not run
    // again, and without a terminal — so it resumes once the vault is unlocked
    assert_eq!(
        (
            daemon.listed_sessions().await,
            daemon.recorded_target_of(&session_id),
            daemon.agent_runs().len(),
            daemon.manager.get(&session_id).await.is_some(),
        ),
        (
            vec![session_id.clone()],
            Some(the_recorded_coder_target()),
            runs_before,
            false,
        )
    );
}
