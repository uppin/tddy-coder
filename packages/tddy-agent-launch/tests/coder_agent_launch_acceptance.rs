//! Acceptance: a session's agent is launched in the **Coder workspace** its target names.
//!
//! The target is resolved to a shell by `shell_for_coder_target_through`, with a stand-in for the
//! vault that holds the Coder account; the shell is the real `CoderShell` behind a stand-in `coder`
//! executable: a script that stands for both the Coder CLI and the workspace it reaches, running
//! what follows `--` on this machine — and a stand-in OpenSSH `ssh` beside it, which a streamed
//! command (the hook follower's) runs through: it runs its `ProxyCommand` (the stand-in `coder`
//! as `coder ssh --stdio`) and then its command here, as the workspace would. No real `ssh` is
//! ever reached. From the shell on, the launch and the hooks are the path an
//! ssh target takes (`remote_agent_launch_acceptance.rs`, `remote_hooks_acceptance.rs`). A temp
//! directory stands for the workspace's filesystem. No deployment is reached.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use tddy_actions::tool::TemplateParams;
use tddy_agent_launch::coder_target::{shell_for_coder_target_through, CoderTargetError};
use tddy_agent_launch::remote_agent_launch::{
    launch_on_target, RemoteLaunch, RemoteLaunchError, RemoteLaunched,
};
use tddy_agent_launch::remote_hook_follower::{create_remote_hook_log, RemoteHookFollower};
use tddy_agent_launch::slot_files_over_shell::write_slot_files_through;
use tddy_agent_launch::spec_launch::HostPtySession;
use tddy_coding_agent_specs::slots::{self, RenderedSlots, SlotInputs};
use tddy_core::{HookCommandParams, SessionActivityStatus, SessionTarget};
use tddy_credentials::SecretString;
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials,
};
use tddy_remote_shell::RemoteShell;
use tddy_testing_commons::wait::eventually;

mod acme_target_support;
mod coder_workspace_support;
use acme_target_support::{
    a_daemon, the_acme_agent, the_acme_spec, ADaemon, InstalledHooks, MODEL,
};
use coder_workspace_support::{a_workspace_answering_one_shots_after, AWorkspace};

/// Who the session's token resolved to: the owner of the vault the account is read from.
const SUBJECT: &str = "ada-on-github";
/// Ada's Coder account, as the accounts listing names it.
const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";
const ADAS_DEPLOYMENT: &str = "https://coder.example.com";
const ADAS_TOKEN: &str = "adas-coder-session-token";
/// The workspace the target names, as `coder ssh` is given it.
const ADAS_DEVBOX: &str = "ada/devbox.main";

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e10";
const SESSION_TYPE: &str = "acme-cli";
/// The session's hook token, as its `.session.yaml` holds it.
const HOOK_TOKEN: &str = "tok-7f3a9c";

/// How long the agent must stay up for a launch to count as started.
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// How long the agent may take to record its pid on the target.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// How long one one-shot takes on a deployment as slow as `coder.ireland-devex.wixprod.net`
/// measured (6–8 s): past the five seconds a probe was once given.
const SLOW_DEPLOYMENT_ROUND_TRIP: &str = "5.5";
/// The startup window of a launch on that deployment: its start bound (ten windows) holds a few
/// of its round trips.
const SLOW_STARTUP_WINDOW: Duration = Duration::from_secs(2);
/// A safety net for a stub to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// What the agent hands its `SessionStart` hook.
const SESSION_START_PAYLOAD: &str =
    r#"{"hook_event_name":"SessionStart","session_id":"c-41","source":"startup"}"#;

/// The target a start names: Ada's workspace `devbox` of her account, on its agent `main`.
fn adas_devbox() -> SessionTarget {
    SessionTarget::coder(ADAS_ACCOUNT, "ada", "devbox").on_agent("main")
}

/// The vault side: answers every read with one fixed outcome, and records who asked for what.
struct AVault {
    answer: Result<CoderCredentials, CoderCredentialError>,
    asked: Mutex<Vec<(String, String)>>,
}

impl CoderCredentialSource for AVault {
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        self.asked
            .lock()
            .expect("the recorder is only held to push")
            .push((subject.to_string(), account_id.to_string()));
        self.answer.clone()
    }
}

impl AVault {
    fn answering(answer: Result<CoderCredentials, CoderCredentialError>) -> Self {
        Self {
            answer,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Who asked this vault for which account, in order.
    fn asked(&self) -> Vec<(String, String)> {
        self.asked.lock().expect("the recorder is readable").clone()
    }
}

fn an_open_vault_holding_adas_account() -> AVault {
    AVault::answering(Ok(CoderCredentials::new(
        ADAS_DEPLOYMENT,
        SecretString::new(ADAS_TOKEN),
    )))
}

fn a_locked_vault() -> AVault {
    AVault::answering(Err(CoderCredentialError::VaultLocked))
}

fn an_open_vault_holding_no_coder_account() -> AVault {
    AVault::answering(Err(CoderCredentialError::NoSuchAccount(
        ADAS_ACCOUNT.to_string(),
    )))
}

/// A running workspace holding the session's worktree ([`coder_workspace_support`]).
fn a_running_workspace() -> AWorkspace {
    coder_workspace_support::a_running_workspace(SESSION_ID)
}

/// A running workspace on a deployment as slow as a real one: every one-shot (`sh -c …`) answers
/// after [`SLOW_DEPLOYMENT_ROUND_TRIP`], longer than any fixed five-second probe; the terminal's
/// command starts at once.
fn a_workspace_on_a_slow_deployment() -> AWorkspace {
    a_workspace_answering_one_shots_after(
        SESSION_ID,
        &format!("if [ \"$1\" = sh ]; then /bin/sleep {SLOW_DEPLOYMENT_ROUND_TRIP}; fi\n"),
    )
}

impl AWorkspace {
    /// The command Claude Code would run for `event`, read from the session's hooks file the
    /// start installed in the worktree in the workspace.
    fn installed_hook(&self, event: &str) -> String {
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                tddy_core::claude_hooks::session_file::claude_session_hooks_file(
                    &self.worktree(),
                    SESSION_ID,
                ),
            )
            .expect("the hooks file is in the worktree in the workspace"),
        )
        .expect("the hooks file is JSON");
        settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_else(|| panic!("the hooks file installs no command for {event}"))
            .to_string()
    }
}

/// A stub agent installed where the workspace finds it.
struct AStubAgent {
    dir: PathBuf,
    _dir: Option<tempfile::TempDir>,
}

impl AStubAgent {
    /// An agent in a directory of this host's: a start that must never run it shows it never did.
    fn running(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the stub agent");
        let path = std::fs::canonicalize(dir.path()).expect("the stub's directory resolves");
        Self::written_to(path, Some(dir), body)
    }

    /// An agent installed where `workspace`'s login finds it, by its bare name.
    fn installed_in(workspace: &AWorkspace, body: &str) -> Self {
        Self::written_to(workspace.agents_dir(), None, body)
    }

    fn written_to(dir: PathBuf, held: Option<tempfile::TempDir>, body: &str) -> Self {
        write_executable(
            &dir.join("acme"),
            &format!("#!/bin/sh\nhere='{}'\n{body}", dir.display()),
        );
        Self { dir, _dir: held }
    }

    fn program(&self) -> PathBuf {
        self.dir.join("acme")
    }

    /// Whether the agent was ever started.
    fn ran(&self) -> bool {
        self.dir.join("cwd").exists()
    }

    /// The directory the agent was started in, once it has recorded it.
    async fn working_directory(&self) -> PathBuf {
        eventually("the agent to record where it was started", OBSERVE, || {
            std::fs::read_to_string(self.dir.join("cwd"))
                .map(|cwd| PathBuf::from(cwd.trim_end()))
                .map_err(|_| "the agent has not run yet".to_string())
        })
        .await
    }
}

/// An agent that records its working directory, then waits on its terminal.
fn an_agent_that_stays_up() -> AStubAgent {
    AStubAgent::running(the_body_of_an_agent_that_stays_up())
}

/// [`an_agent_that_stays_up`] as installed in `workspace`.
fn an_agent_that_stays_up_in(workspace: &AWorkspace) -> AStubAgent {
    AStubAgent::installed_in(workspace, the_body_of_an_agent_that_stays_up())
}

fn the_body_of_an_agent_that_stays_up() -> &'static str {
    r#"/bin/pwd -P > "$here/cwd"
printf 'ACME READY in the workspace\n'
exec /bin/cat
"#
}

/// An agent that fires its `SessionStart` hook as it starts — `hook_command` under `sh -c`, with
/// [`SESSION_START_PAYLOAD`] on its stdin, as Claude Code runs a hook — then waits on its terminal.
fn an_agent_that_fires_as_it_starts_in(workspace: &AWorkspace, hook_command: &str) -> AStubAgent {
    let agent = AStubAgent::installed_in(
        workspace,
        &format!(
            r#"PATH=/usr/bin:/bin
export PATH
/bin/pwd -P > "$here/cwd"
printf '%s' '{SESSION_START_PAYLOAD}' | /bin/sh -c "$(/bin/cat "$here/session-start-hook")"
printf 'ACME READY in the workspace\n'
exec /bin/cat
"#
        ),
    );
    std::fs::write(agent.dir.join("session-start-hook"), hook_command)
        .expect("the hook command is kept for the stub");
    agent
}

/// Why a start on a Coder target did not start the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AStartFailure {
    /// The target was not resolved to a shell.
    Target(CoderTargetError),
    /// The shell was resolved and the launch through it failed.
    Launch(RemoteLaunchError),
}

impl std::fmt::Display for AStartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Target(e) => write!(f, "{e}"),
            Self::Launch(e) => write!(f, "{e}"),
        }
    }
}

impl ADaemon {
    /// The shell that reaches `target`, asked as [`SUBJECT`] with their `vault`, through
    /// `workspace`'s stand-in `coder` and `ssh`.
    fn the_shell_for(
        &self,
        target: &SessionTarget,
        workspace: &AWorkspace,
        vault: &AVault,
    ) -> Result<Arc<dyn RemoteShell>, CoderTargetError> {
        shell_for_coder_target_through(workspace.coder(), workspace.ssh(), target, SUBJECT, vault)
    }

    /// Launch `agent` through `shell` in the session's worktree in `workspace`.
    async fn launches_through(
        &self,
        shell: Arc<dyn RemoteShell>,
        agent: &AStubAgent,
        workspace: &AWorkspace,
        slots: &RenderedSlots,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_within(shell, agent, workspace, slots, STARTUP_WINDOW)
            .await
    }

    /// [`Self::launches_through`], counted as started once the agent outlives `startup_window`.
    async fn launches_within(
        &self,
        shell: Arc<dyn RemoteShell>,
        agent: &AStubAgent,
        workspace: &AWorkspace,
        slots: &RenderedSlots,
        startup_window: Duration,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        launch_on_target(
            &the_acme_agent(&agent.program()),
            &TemplateParams::new().with("model", MODEL),
            slots,
            RemoteLaunch {
                shell,
                worktree: &workspace.worktree(),
                session_dir: &self.session_dir,
                pointer: None,
                startup_window,
                start_timeout: START_TIMEOUT,
                clone_root: None,
                tmux: false,
            },
            HostPtySession {
                manager: &self.manager,
                session_id: SESSION_ID,
                session_type: SESSION_TYPE,
                model: MODEL,
                env: Vec::new(),
                os_user: None,
            },
            None,
        )
        .await
    }

    /// What a start on `target` does: resolve the target to a shell with the caller's `vault`,
    /// then launch `agent` through it.
    async fn starts_on(
        &self,
        target: &SessionTarget,
        agent: &AStubAgent,
        workspace: &AWorkspace,
        vault: &AVault,
    ) -> Result<RemoteLaunched, AStartFailure> {
        let shell = self
            .the_shell_for(target, workspace, vault)
            .map_err(AStartFailure::Target)?;
        self.launches_through(shell, agent, workspace, &RenderedSlots::default())
            .await
            .map_err(AStartFailure::Launch)
    }

    /// What a start does before it launches the agent: create the hook log in the workspace,
    /// render the agent's hooks to log there, and write the hooks file into the worktree there.
    async fn installs_the_sessions_hooks_through(
        &self,
        shell: &dyn RemoteShell,
        workspace: &AWorkspace,
    ) -> InstalledHooks {
        let log = create_remote_hook_log(shell, &workspace.worktree())
            .await
            .expect("the hook log is created in the workspace");
        let slots = slots::render(
            &the_acme_spec(),
            &SlotInputs {
                workdir: workspace.worktree(),
                scratch_dir: self.session_dir.clone(),
                mcp_server: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
                permission_prompt_tool: None,
                preamble: None,
                append_system_prompt_file: None,
                hooks: Some(HookCommandParams {
                    tddy_tools_path: "/usr/local/bin/tddy-tools",
                    daemon_url: "http://127.0.0.1:8899",
                    session_id: SESSION_ID,
                    os_user: "dev",
                    hook_token: HOOK_TOKEN,
                    daemon_socket: None,
                    answer_wait_secs: 120,
                    remote_log: Some(&log),
                    route_env: &[],
                }),
                home: None,
            },
        )
        .expect("the agent's slots render for a target");
        write_slot_files_through(shell, &slots.files)
            .await
            .expect("the hooks file is written in the workspace");
        InstalledHooks { slots, log }
    }

    /// Whether the session has a terminal.
    async fn has_a_terminal(&self) -> bool {
        self.manager.get(SESSION_ID).await.is_some()
    }

    /// The session's activity once a hook has set it.
    async fn activity_once_a_hook_arrives(&self) -> Vec<SessionActivityStatus> {
        eventually("a hook to change the session's activity", OBSERVE, || {
            let statuses = self.activity.statuses.lock().unwrap().clone();
            (!statuses.is_empty())
                .then_some(statuses)
                .ok_or_else(|| "no hook has reached the daemon".to_string())
        })
        .await
    }
}

#[tokio::test]
async fn the_agent_runs_in_the_coder_workspace_named_by_the_target() {
    // Given a running workspace holding the session's worktree, the agent installed there, and
    // the caller's open vault holding the account the target names
    let workspace = a_running_workspace();
    let agent = an_agent_that_stays_up_in(&workspace);
    let vault = an_open_vault_holding_adas_account();
    let daemon = a_daemon();

    // When a session is started on the target
    let started = daemon
        .starts_on(&adas_devbox(), &agent, &workspace, &vault)
        .await
        .map(|_launched| ());
    assert_eq!(started, Ok(()));
    let agent_runs_in = agent.working_directory().await;

    // Then the agent runs in the session's worktree in the workspace
    assert_eq!(agent_runs_in, workspace.worktree());

    // And the CLI that carries it ran once, in the session's directory, for that workspace's
    // agent, as the account the caller's vault holds
    let launches = workspace.calls_carrying(&format!("> '.tddy/agents/{SESSION_ID}.pid'"));
    assert_eq!(
        launches
            .iter()
            .map(|call| (
                call.cwd.clone(),
                call.args.contains(ADAS_DEVBOX),
                call.deployment.as_str(),
                call.token.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            daemon.session_dir.clone(),
            true,
            ADAS_DEPLOYMENT,
            ADAS_TOKEN
        )]
    );
    assert_eq!(
        vault.asked(),
        vec![(SUBJECT.to_string(), ADAS_ACCOUNT.to_string())]
    );
}

#[tokio::test]
async fn a_hook_fired_in_a_coder_workspace_reaches_the_daemon() {
    // Given a session on a Coder target whose hooks are installed in its worktree in the
    // workspace, and an agent there that fires its SessionStart hook as it starts
    let workspace = a_running_workspace();
    let vault = an_open_vault_holding_adas_account();
    let daemon = a_daemon();
    let shell = daemon
        .the_shell_for(&adas_devbox(), &workspace, &vault)
        .expect("a Coder target whose vault is open resolves to a shell");
    let hooks = daemon
        .installs_the_sessions_hooks_through(shell.as_ref(), &workspace)
        .await;
    let agent =
        an_agent_that_fires_as_it_starts_in(&workspace, &workspace.installed_hook("SessionStart"));

    // When the agent is launched and the session's hook log is followed
    let _launched = daemon
        .launches_through(Arc::clone(&shell), &agent, &workspace, &hooks.slots)
        .await
        .expect("the agent is launched in the workspace");
    let follower =
        RemoteHookFollower::start(shell, &hooks.log, HOOK_TOKEN, daemon.activity.clone());
    let activity = daemon.activity_once_a_hook_arrives().await;

    // Then the session has started, as a local SessionStart hook would have reported it
    assert_eq!(activity, vec![SessionActivityStatus::Started]);
    follower.stop().await;
}

#[tokio::test]
async fn a_target_whose_vault_is_locked_fails_the_start_and_says_so() {
    // Given a running workspace, and a caller whose vault is locked
    let workspace = a_running_workspace();
    let agent = an_agent_that_stays_up();
    let vault = a_locked_vault();
    let daemon = a_daemon();

    // When a session is started on the target
    let failure = daemon
        .starts_on(&adas_devbox(), &agent, &workspace, &vault)
        .await
        .err();

    // Then the start fails because the vault is locked, and says how to go on
    assert_eq!(
        (failure.clone(), failure.map(|failure| failure.to_string()),),
        (
            Some(AStartFailure::Target(CoderTargetError::VaultLocked)),
            Some(
                "the vault holding this session's Coder account is locked: unlock it and try \
                 again"
                    .to_string()
            ),
        )
    );

    // And nothing was attempted: the CLI never ran, nor the agent, and no terminal is left
    assert_eq!(
        (
            workspace.calls(),
            agent.ran(),
            daemon.has_a_terminal().await
        ),
        (Vec::new(), false, false)
    );
}

#[tokio::test]
async fn a_target_naming_an_unknown_account_is_refused() {
    // Given a running workspace, and a caller whose open vault holds no such Coder account
    let workspace = a_running_workspace();
    let agent = an_agent_that_stays_up();
    let vault = an_open_vault_holding_no_coder_account();
    let daemon = a_daemon();

    // When a session is started on the target
    let failure = daemon
        .starts_on(&adas_devbox(), &agent, &workspace, &vault)
        .await
        .err();

    // Then it is refused, naming the account the vault does not hold
    assert_eq!(
        failure,
        Some(AStartFailure::Target(CoderTargetError::NoSuchAccount(
            ADAS_ACCOUNT.to_string()
        )))
    );

    // And nothing was attempted: the CLI never ran, nor the agent, and no terminal is left
    assert_eq!(
        (
            workspace.calls(),
            agent.ran(),
            daemon.has_a_terminal().await
        ),
        (Vec::new(), false, false)
    );
}

#[tokio::test]
async fn an_agent_on_a_deployment_slower_than_five_seconds_a_call_is_launched() {
    // Given a workspace whose every one-shot takes longer than five seconds, as a real
    // deployment's does, and an agent there that stays up
    let workspace = a_workspace_on_a_slow_deployment();
    let agent = an_agent_that_stays_up_in(&workspace);
    let vault = an_open_vault_holding_adas_account();
    let daemon = a_daemon();
    let shell = daemon
        .the_shell_for(&adas_devbox(), &workspace, &vault)
        .expect("a Coder target whose vault is open resolves to a shell");

    // When the agent is launched
    let launched = daemon
        .launches_within(
            shell,
            &agent,
            &workspace,
            &RenderedSlots::default(),
            SLOW_STARTUP_WINDOW,
        )
        .await
        .map(|_launched| ());

    // Then it is launched — the probes that judge its start wait for the deployment's answers —
    // and it runs in the session's worktree
    assert_eq!(launched, Ok(()));
    assert_eq!(agent.working_directory().await, workspace.worktree());
}

#[tokio::test]
async fn an_agent_that_exits_on_a_slow_deployment_fails_its_launch_with_its_last_words() {
    // Given a workspace whose every one-shot takes longer than five seconds, and an agent there
    // that exits as it starts
    let workspace = a_workspace_on_a_slow_deployment();
    let agent = AStubAgent::installed_in(
        &workspace,
        "printf 'acme: no licence for this host\\n'\nexit 3\n",
    );
    let vault = an_open_vault_holding_adas_account();
    let daemon = a_daemon();
    let shell = daemon
        .the_shell_for(&adas_devbox(), &workspace, &vault)
        .expect("a Coder target whose vault is open resolves to a shell");

    // When the agent is launched
    let launched = daemon
        .launches_within(
            shell,
            &agent,
            &workspace,
            &RenderedSlots::default(),
            SLOW_STARTUP_WINDOW,
        )
        .await
        .map(|_launched| ());

    // Then the launch fails with what the agent said as it exited
    assert!(
        matches!(&launched, Err(RemoteLaunchError::ExitedDuringStartup { last_output, .. })
            if last_output.contains("acme: no licence for this host")),
        "expected the launch to fail with the agent's last words, got {launched:?}"
    );
}
