//! Acceptance: the hooks of an agent that runs on a target reach the daemon, with nothing of
//! tddy's on the target.
//!
//! As in `remote_agent_launch_acceptance.rs`, the port is the real `SshShell` behind a stand-in
//! `ssh` executable that runs the remote command on this machine, and a temp directory stands for
//! the target's filesystem. The agent is a stub that, as Claude Code does on a `SessionStart`,
//! hands the event's payload to the hook command installed in its worktree, under `sh -c`. The
//! daemon's end is a [`HookSink`] that keeps the session's activity as a local hook's report would
//! set it. Nothing opens a connection.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use tddy_actions::tool::TemplateParams;
use tddy_agent_launch::remote_agent_launch::{launch_on_target, RemoteLaunch, RemoteLaunched};
use tddy_agent_launch::remote_hook_follower::{
    create_remote_hook_log, remove_remote_hook_log, RemoteHookFollower,
};
use tddy_agent_launch::slot_files_over_shell::write_slot_files_through;
use tddy_agent_launch::spec_launch::HostPtySession;
use tddy_coding_agent_specs::slots::{self, SlotInputs};
use tddy_core::{HookCommandParams, SessionActivityStatus, REMOTE_HOOK_LOG};
use tddy_remote_shell::{RemoteShellError, SshShell};
use tddy_testing_commons::wait::eventually;

mod acme_target_support;
use acme_target_support::{
    a_daemon, the_acme_agent, the_acme_spec, ADaemon, InstalledHooks, MODEL,
};

/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e10";
const SESSION_TYPE: &str = "acme-cli";
/// The session's hook token, as its `.session.yaml` holds it.
const HOOK_TOKEN: &str = "tok-7f3a9c";

/// How long the agent must stay up for a launch to count as started.
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// How long the agent may take to record its pid on the target.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// A safety net for a stub to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);
/// How long the session's waiting hooks wait for an answer: longer than [`OBSERVE`], so a hook that
/// ends within [`OBSERVE`] ended because it was answered, not because it gave up.
const ANSWER_WAIT_SECS: u64 = 30;

/// What the agent hands its `SessionStart` hook.
const SESSION_START_PAYLOAD: &str =
    r#"{"hook_event_name":"SessionStart","session_id":"c-41","source":"startup"}"#;

/// A machine the daemon reaches over ssh: a directory standing for its filesystem, holding the
/// session's worktree, and the stand-in `ssh` that reaches it.
struct ATarget {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

/// A target that runs what it is asked to: the stand-in runs its last argument — the remote
/// command — with `sh -c` as a child of its own, its stdin passed on. It hangs the command up
/// when it is itself hung up or terminated, and exits with the command's status. Every command
/// that has ended is recorded in `<root>/ran`, so a test can tell when the daemon's work is done.
fn a_reachable_target() -> ATarget {
    let dir = tempfile::tempdir().expect("a directory standing for the target");
    let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
    std::fs::create_dir_all(root.join("bin")).expect("a directory for the stand-in ssh");
    write_executable(
        &root.join("bin/ssh"),
        r#"#!/bin/sh
for command; do :; done
SHELL="$(dirname "$0")/login-shell"
export SHELL
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
status=$?
printf '%s\n' "$command" >> "$(dirname "$0")/../ran"
exit $status
"#,
    );
    let target = ATarget { root, _dir: dir };
    for path in [target.worktree(), target.agents_dir()] {
        std::fs::create_dir_all(path).expect("a directory of the target");
    }
    // The target's login shell finds its agents on a `PATH` of its own.
    write_executable(
        &target.root.join("bin/login-shell"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\nPATH='{}:/usr/bin:/bin'\nexport PATH\n\
             exec /bin/sh -c \"$2\"\n",
            target.agents_dir().display()
        ),
    );
    target
}

impl ATarget {
    /// The remote shell on the target, through the stand-in.
    fn shell(&self) -> Arc<SshShell> {
        Arc::new(SshShell::through(self.root.join("bin/ssh"), TARGET_ALIAS))
    }

    /// Where the target's login finds its agents: the first directory of its `PATH`.
    fn agents_dir(&self) -> PathBuf {
        self.root.join("opt/agents")
    }

    /// The session's worktree on the target.
    fn worktree(&self) -> PathBuf {
        self.root.join("home/dev/repo/.worktrees").join(SESSION_ID)
    }

    /// Where the session's hook log lives on the target.
    fn hook_log(&self) -> PathBuf {
        self.worktree().join(REMOTE_HOOK_LOG)
    }

    /// A hook log an earlier start left in the worktree, holding one hook's line.
    fn holds_a_hook_log(&self) {
        let log = self.hook_log();
        std::fs::create_dir_all(log.parent().expect("the log has a directory"))
            .expect("the log's directory on the target");
        std::fs::write(
            &log,
            format!("SessionStart\t{HOOK_TOKEN}\t{SESSION_START_PAYLOAD}\n"),
        )
        .expect("the hook log is on the target");
    }

    /// The command Claude Code would run for `event` on a call of `tool`, read from the session's
    /// hooks file the launch installed in the worktree on the target: the entry whose matcher covers the
    /// tool (`""`, `"*"`, the tool's name, or an alternation naming it). `None` when no entry does.
    fn installed_hook_for_tool(&self, event: &str, tool: &str) -> Option<String> {
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                tddy_core::claude_hooks::session_file::claude_session_hooks_file(
                    &self.worktree(),
                    SESSION_ID,
                ),
            )
            .expect("the hooks file is in the worktree on the target"),
        )
        .expect("the hooks file is JSON");
        // A group naming the tool comes before a catch-all one: `PreToolUse` carries both for a
        // question tool (`#target-parity` 4/7, 6/7), and the named one is the one that waits.
        let groups = settings["hooks"][event].as_array()?;
        let names_it = |entry: &&serde_json::Value| {
            entry["matcher"]
                .as_str()
                .unwrap_or("")
                .split('|')
                .any(|m| m == tool)
        };
        let matches_all = |entry: &&serde_json::Value| {
            matches!(entry["matcher"].as_str().unwrap_or(""), "" | "*")
        };
        groups
            .iter()
            .find(names_it)
            .or_else(|| groups.iter().find(matches_all))
            .and_then(|entry| entry["hooks"][0]["command"].as_str())
            .map(str::to_string)
    }

    /// The answers the daemon wrote for the session's waiting hooks that are still on the target.
    fn answers_left(&self) -> Vec<String> {
        std::fs::read_dir(self.worktree().join(tddy_core::REMOTE_ANSWERS_DIR))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Once a command that writes an answer into the session's answers directory has ended.
    async fn once_an_answer_is_written(&self) {
        eventually(
            "the daemon to write an answer on the target",
            OBSERVE,
            || {
                std::fs::read_to_string(self.root.join("ran"))
                    .unwrap_or_default()
                    .lines()
                    .any(|command| {
                        command.contains(tddy_core::REMOTE_ANSWERS_DIR) && command.contains(" mv ")
                    })
                    .then_some(())
                    .ok_or_else(|| "no answer has been written on the target".to_string())
            },
        )
        .await;
    }

    /// The command Claude Code would run for `event`, read from the session's hooks file the
    /// launch installed in the worktree on the target.
    fn installed_hook(&self, event: &str) -> String {
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                tddy_core::claude_hooks::session_file::claude_session_hooks_file(
                    &self.worktree(),
                    SESSION_ID,
                ),
            )
            .expect("the hooks file is in the worktree on the target"),
        )
        .expect("the hooks file is JSON");
        settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_else(|| panic!("the hooks file installs no command for {event}"))
            .to_string()
    }
}

/// A stub agent installed where the target finds it.
struct AStubAgent {
    dir: PathBuf,
}

/// An agent that fires its `SessionStart` hook as it starts — `hook_command` under `sh -c`, with
/// [`SESSION_START_PAYLOAD`] on its stdin, as Claude Code runs a hook — then waits on its terminal.
fn an_agent_that_fires_as_it_starts(target: &ATarget, hook_command: &str) -> AStubAgent {
    an_agent_that_fires_as_it_starts_with(target, hook_command, SESSION_START_PAYLOAD)
}

/// [`an_agent_that_fires_as_it_starts`], handing the hook `payload` instead.
fn an_agent_that_fires_as_it_starts_with(
    target: &ATarget,
    hook_command: &str,
    payload: &str,
) -> AStubAgent {
    let path = target.agents_dir();
    std::fs::write(path.join("hook-payload"), payload).expect("the payload is kept for the stub");
    std::fs::write(path.join("session-start-hook"), hook_command)
        .expect("the hook command is kept for the stub");
    write_executable(
        &path.join("acme"),
        &format!(
            r#"#!/bin/sh
here='{here}'
PATH=/usr/bin:/bin
export PATH
/bin/cat "$here/hook-payload" | /bin/sh -c "$(/bin/cat "$here/session-start-hook")" > "$here/hook-output"
: > "$here/hook-done"
printf 'ACME READY on the target\n'
exec /bin/cat
"#,
            here = path.display()
        ),
    );
    AStubAgent { dir: path }
}

impl AStubAgent {
    fn program(&self) -> PathBuf {
        self.dir.join("acme")
    }

    /// What the hook printed — the decision Claude Code would read — once the hook has ended.
    async fn hook_output_once_it_ends(&self) -> String {
        eventually("the agent's hook to end", OBSERVE, || {
            self.dir
                .join("hook-done")
                .exists()
                .then_some(())
                .ok_or_else(|| "the hook is still running".to_string())
        })
        .await;
        std::fs::read_to_string(self.dir.join("hook-output")).expect("the hook's output is kept")
    }
}

/// A target session that is running: its terminal and the follower of its hook log.
struct ARunningSession {
    _launched: RemoteLaunched,
    follower: RemoteHookFollower,
}

impl ADaemon {
    /// What a start does before it launches the agent: create the hook log on the target, render
    /// the agent's hooks to log there, and write the hooks file into the worktree on the target.
    async fn installs_the_sessions_hooks_on(&self, target: &ATarget) -> InstalledHooks {
        self.installs_the_sessions_hooks_waiting(target, ANSWER_WAIT_SECS)
            .await
    }

    /// [`Self::installs_the_sessions_hooks_on`], with waiting hooks that wait `answer_wait_secs` for
    /// their orchestrator's answer.
    async fn installs_the_sessions_hooks_waiting(
        &self,
        target: &ATarget,
        answer_wait_secs: u64,
    ) -> InstalledHooks {
        let shell = target.shell();
        let log = create_remote_hook_log(shell.as_ref(), &target.worktree())
            .await
            .expect("the hook log is created on the target");
        let slots = slots::render(
            &the_acme_spec(),
            &SlotInputs {
                workdir: target.worktree(),
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
                    answer_wait_secs,
                    remote_log: Some(&log),
                    route_env: &[],
                }),
                home: None,
            },
        )
        .expect("the agent's slots render for a target");
        write_slot_files_through(shell.as_ref(), &slots.files)
            .await
            .expect("the hooks file is written on the target");
        InstalledHooks { slots, log }
    }

    /// Launch `agent` on `target` and follow the session's hook log from then on.
    async fn launches(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
        hooks: &InstalledHooks,
    ) -> ARunningSession {
        let launched = launch_on_target(
            &the_acme_agent(&agent.program()),
            &TemplateParams::new().with("model", MODEL),
            &hooks.slots,
            RemoteLaunch {
                shell: target.shell(),
                worktree: &target.worktree(),
                session_dir: &self.session_dir,
                pointer: None,
                startup_window: STARTUP_WINDOW,
                start_timeout: START_TIMEOUT,
                clone_root: None,
                tmux: false,
                tmux_location: None,
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
        .expect("the agent is launched on the target");
        ARunningSession {
            _launched: launched,
            follower: self.follows(target, &hooks.log),
        }
    }

    /// Follow the hook log at `log` on `target` into the session's activity.
    fn follows(&self, target: &ATarget, log: &str) -> RemoteHookFollower {
        RemoteHookFollower::start(target.shell(), log, HOOK_TOKEN, self.activity.clone())
    }

    /// What deleting the session does to its hooks: stop the follower, then remove the log.
    async fn deletes_the_session(
        &self,
        follower: RemoteHookFollower,
        target: &ATarget,
    ) -> Result<(), RemoteShellError> {
        follower.stop().await;
        remove_remote_hook_log(target.shell().as_ref(), &target.worktree()).await
    }

    /// Every hook delivered to the session once `event` has been, in order.
    async fn hooks_once_delivered(&self, event: &str) -> Vec<(String, serde_json::Value)> {
        eventually(
            &format!("a {event} hook to reach the daemon"),
            OBSERVE,
            || {
                let delivered: Vec<(String, serde_json::Value)> = self
                    .activity
                    .delivered
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|line| (line.event.clone(), line.payload.clone()))
                    .collect();
                delivered
                    .iter()
                    .any(|(name, _)| name == event)
                    .then_some(delivered.clone())
                    .ok_or_else(|| format!("no {event} hook has reached the daemon: {delivered:?}"))
            },
        )
        .await
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
async fn a_hook_fired_by_an_agent_on_a_target_updates_the_sessions_activity() {
    // Given a target session whose hooks are installed in its worktree on the target, and an agent
    // there that fires its SessionStart hook as it starts
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let agent = an_agent_that_fires_as_it_starts(&target, &target.installed_hook("SessionStart"));

    // When
    let session = daemon.launches(&agent, &target, &hooks).await;
    let activity = daemon.activity_once_a_hook_arrives().await;

    // Then the session has started, as a local SessionStart hook would have reported it
    assert_eq!(activity, vec![SessionActivityStatus::Started]);
    session.follower.stop().await;
}

#[tokio::test]
async fn deleting_the_session_removes_the_hook_log_on_the_target() {
    // Given a target session whose hook log is on the target and being followed
    let target = a_reachable_target();
    let daemon = a_daemon();
    target.holds_a_hook_log();
    let follower = daemon.follows(&target, &target.hook_log().to_string_lossy());

    // When
    let deleted = daemon.deletes_the_session(follower, &target).await;

    // Then
    assert_eq!((deleted, target.hook_log().exists()), (Ok(()), false));
}

/// What Claude Code hands its `PermissionRequest` hook when it wants to read a file, as the probe
/// session `01a1fcba-…` on the Coder workspace `mantasi/remote-dev` asked (2026-11-21).
const PERMISSION_REQUEST_PAYLOAD: &str = r#"{"hook_event_name":"PermissionRequest","session_id":"c-41","tool_name":"Read","tool_input":{"file_path":"README.md","limit":5}}"#;

/// Reproduces gap 1 of the 2026-11-21 Coder probe: a permission prompt of an agent on a target is
/// never reported, so the session has no pending question and `AnswerSessionPermission` has
/// nothing to answer. `build_claude_hooks_settings` leaves `PermissionRequest` out of a target's
/// settings (`tddy-agent-backend/src/claude_hooks.rs:181`), so the agent asks in its terminal and
/// the daemon is told only by the `Notification` that follows, which carries no tool or input.
#[tokio::test]
async fn a_permission_request_of_an_agent_on_a_target_reaches_the_daemon_with_its_tool_and_input() {
    // Given a target session whose hooks are installed in its worktree on the target
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let hook = target
        .installed_hook_for_tool("PermissionRequest", "Read")
        .expect("the hooks file on the target installs a PermissionRequest hook for Read");
    // And an agent there that asks for permission to read a file as it starts
    let agent = an_agent_that_fires_as_it_starts_with(&target, &hook, PERMISSION_REQUEST_PAYLOAD);

    // When
    let session = daemon.launches(&agent, &target, &hooks).await;
    let delivered = daemon.hooks_once_delivered("PermissionRequest").await;

    // Then the daemon is told what is asked: the tool and its input
    let asked = delivered
        .iter()
        .find(|(event, _)| event == "PermissionRequest")
        .map(|(_, payload)| (payload["tool_name"].clone(), payload["tool_input"].clone()));
    assert_eq!(
        asked,
        Some((
            serde_json::json!("Read"),
            serde_json::json!({"file_path": "README.md", "limit": 5})
        ))
    );
    session.follower.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Answering a target agent's permission prompt (`#target-parity` 1/7)
// ---------------------------------------------------------------------------------------------

/// The request id the daemon records a target agent's permission prompt under.
const REQUEST_ID: &str = "req-7f3a";

/// The daemon's end of a target session whose orchestrator answers: a permission or a question is
/// recorded under [`REQUEST_ID`], and the orchestrator's answer to it is `answer`.
struct AnOrchestrator {
    answer: tddy_service::proto::activity::AwaitQuestionAnswerResponse,
    asked: std::sync::Mutex<Vec<tddy_service::proto::activity::AwaitQuestionAnswerRequest>>,
    /// How long the orchestrator takes to answer once it is asked.
    takes: Duration,
}

impl AnOrchestrator {
    fn answering(option_id: &str, message: &str) -> Arc<Self> {
        Self::with(tddy_service::proto::activity::AwaitQuestionAnswerResponse {
            outcome: tddy_service::proto::activity::AwaitQuestionOutcome::Answered as i32,
            option_id: option_id.to_string(),
            message: message.to_string(),
            answered_by: "home-session".to_string(),
            ..Default::default()
        })
    }

    /// No orchestrator is there to answer: the daemon says so at once.
    fn absent() -> Arc<Self> {
        Self::with(tddy_service::proto::activity::AwaitQuestionAnswerResponse {
            outcome: tddy_service::proto::activity::AwaitQuestionOutcome::NoOrchestrator as i32,
            ..Default::default()
        })
    }

    /// The orchestrator answers the agent's question `question_index` with `selected`.
    fn choosing(question_index: u32, selected: &str) -> Arc<Self> {
        Self::with(tddy_service::proto::activity::AwaitQuestionAnswerResponse {
            outcome: tddy_service::proto::activity::AwaitQuestionOutcome::Answered as i32,
            answered_by: "home-session".to_string(),
            answers: vec![tddy_service::proto::activity::QuestionAnswer {
                question_index,
                selected: vec![selected.to_string()],
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    fn with(answer: tddy_service::proto::activity::AwaitQuestionAnswerResponse) -> Arc<Self> {
        Arc::new(Self {
            answer,
            asked: std::sync::Mutex::new(Vec::new()),
            takes: Duration::ZERO,
        })
    }

    /// The orchestrator allows what is asked, but only `takes` after it is asked.
    fn allowing_after(takes: Duration) -> Arc<Self> {
        let allowing = Self::answering("allow_once", "");
        Arc::new(Self {
            answer: allowing.answer.clone(),
            asked: std::sync::Mutex::new(Vec::new()),
            takes,
        })
    }

    /// The request ids the daemon held for an answer to.
    fn held_for(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.request_id.clone())
            .collect()
    }
}

#[async_trait::async_trait]
impl tddy_agent_launch::remote_hook_follower::HookReportDelivery for AnOrchestrator {
    async fn report_session_status(
        &self,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<
        tddy_service::proto::activity::ReportSessionStatusResponse,
        tddy_agent_launch::remote_hook_follower::HookDeliveryError,
    > {
        let asks = !request.tool_name.is_empty();
        Ok(tddy_service::proto::activity::ReportSessionStatusResponse {
            ok: true,
            request_id: if asks {
                REQUEST_ID.to_string()
            } else {
                String::new()
            },
        })
    }

    async fn report_agent_activity(
        &self,
        _request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), tddy_agent_launch::remote_hook_follower::HookDeliveryError> {
        Ok(())
    }

    async fn await_question_answer(
        &self,
        request: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
    ) -> Result<
        tddy_service::proto::activity::AwaitQuestionAnswerResponse,
        tddy_agent_launch::remote_hook_follower::HookDeliveryError,
    > {
        self.asked.lock().unwrap().push(request);
        tokio::time::sleep(self.takes).await;
        Ok(self.answer.clone())
    }
}

impl ADaemon {
    /// Launch `agent` on `target` and follow the session's hook log with the production sink,
    /// answering through the session's shell, in front of `orchestrator`.
    async fn launches_answered_by(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
        hooks: &InstalledHooks,
        orchestrator: Arc<AnOrchestrator>,
    ) -> ARunningSession {
        self.launches_answered_by_waiting(agent, target, hooks, orchestrator, ANSWER_WAIT_SECS)
            .await
    }

    /// [`Self::launches_answered_by`], for a session whose waiting hooks wait `answer_wait_secs`.
    async fn launches_answered_by_waiting(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
        hooks: &InstalledHooks,
        orchestrator: Arc<AnOrchestrator>,
        answer_wait_secs: u64,
    ) -> ARunningSession {
        let mut session = self.launches(agent, target, hooks).await;
        session.follower.stop().await;
        let sink = tddy_agent_launch::remote_hook_follower::ActivityHookSink::new(
            orchestrator,
            SESSION_ID,
            HOOK_TOKEN,
            "dev",
        )
        .answering_through(target.shell(), &target.worktree(), answer_wait_secs);
        session.follower =
            RemoteHookFollower::start(target.shell(), &hooks.log, HOOK_TOKEN, Arc::new(sink));
        session
    }
}

/// An agent on `target` that asks for permission to read a file as it starts, through the waiting
/// `PermissionRequest` hook the session installed there.
fn an_agent_asking_to_read_a_file(target: &ATarget) -> AStubAgent {
    let hook = target
        .installed_hook_for_tool("PermissionRequest", "Read")
        .expect("the hooks file on the target installs a PermissionRequest hook for Read");
    an_agent_that_fires_as_it_starts_with(target, &hook, PERMISSION_REQUEST_PAYLOAD)
}

#[tokio::test]
async fn an_orchestrators_answer_to_a_target_agents_permission_request_is_printed_by_its_hook() {
    // Given a target session whose orchestrator allows what is asked
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let orchestrator = AnOrchestrator::answering("allow_once", "");
    // And an agent there that asks to read a file as it starts
    let agent = an_agent_asking_to_read_a_file(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, Arc::clone(&orchestrator))
        .await;
    let printed = agent.hook_output_once_it_ends().await;

    // Then the hook printed Claude's allow decision for the request the daemon recorded
    assert_eq!(
        (
            serde_json::from_str::<serde_json::Value>(printed.trim()).ok(),
            orchestrator.held_for()
        ),
        (
            Some(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {"behavior": "allow"}
                }
            })),
            vec![REQUEST_ID.to_string()]
        )
    );
    session.follower.stop().await;
}

#[tokio::test]
async fn an_orchestrators_denial_reaches_the_target_agent_with_its_message() {
    // Given a target session whose orchestrator refuses what is asked, saying why
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let orchestrator = AnOrchestrator::answering("reject_once", "read the spec instead");
    let agent = an_agent_asking_to_read_a_file(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, orchestrator)
        .await;
    let printed = agent.hook_output_once_it_ends().await;

    // Then
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(printed.trim()).ok(),
        Some(serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": {"behavior": "deny", "message": "read the spec instead"}
            }
        }))
    );
    session.follower.stop().await;
}

#[tokio::test]
async fn a_permission_request_nobody_answers_leaves_the_target_agent_to_ask_in_its_terminal() {
    // Given a target session with no orchestrator to answer
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let agent = an_agent_asking_to_read_a_file(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, AnOrchestrator::absent())
        .await;

    // Then the hook ends at once — well inside its own wait — and prints no decision, so Claude
    // asks in its terminal
    assert_eq!(agent.hook_output_once_it_ends().await, "");
    session.follower.stop().await;
}

#[tokio::test]
async fn ending_a_target_session_removes_its_answers_on_the_target() {
    // Given a target session whose agent's permission request was answered
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let agent = an_agent_asking_to_read_a_file(&target);
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, AnOrchestrator::absent())
        .await;
    agent.hook_output_once_it_ends().await;
    let answers = target.worktree().join(tddy_core::REMOTE_ANSWERS_DIR);
    std::fs::create_dir_all(&answers).expect("the answers directory on the target");
    std::fs::write(answers.join("left-over"), "").expect("an answer left on the target");

    // When the session ends
    let ended = daemon.deletes_the_session(session.follower, &target).await;

    // Then neither the hook log nor the answers are left on the target
    assert_eq!(
        (ended, target.hook_log().exists(), answers.exists()),
        (Ok(()), false, false)
    );
}

/// A waiting hook's answer wait short enough for a test to outlast: the hook gives up after this
/// plus `AWAIT_CLIENT_MARGIN_SECS`.
const SHORT_ANSWER_WAIT_SECS: u64 = 1;

/// An answer that reaches the target after its hook gave up — a Coder write takes 6–9 s — is
/// never read: the hook removes only an answer it reads. Nothing else removed it before the
/// session ended (`#target-parity` sweep).
#[tokio::test]
async fn an_answer_written_after_the_hook_gave_up_is_not_left_on_the_target() {
    // Given a target session whose waiting hooks wait briefly
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon
        .installs_the_sessions_hooks_waiting(&target, SHORT_ANSWER_WAIT_SECS)
        .await;
    // And an orchestrator that answers only once the hook has given up
    let hook_gives_up_after = SHORT_ANSWER_WAIT_SECS + tddy_core::AWAIT_CLIENT_MARGIN_SECS + 1;
    let orchestrator = AnOrchestrator::allowing_after(Duration::from_secs(hook_gives_up_after + 3));
    let agent = an_agent_asking_to_read_a_file(&target);

    // When the agent asks, its hook gives up, and the answer is written afterwards
    let session = daemon
        .launches_answered_by_waiting(
            &agent,
            &target,
            &hooks,
            orchestrator,
            SHORT_ANSWER_WAIT_SECS,
        )
        .await;
    let printed = agent.hook_output_once_it_ends().await;
    target.once_an_answer_is_written().await;

    // Then the hook printed no decision
    assert_eq!(printed, "");
    // And once its window has passed no answer is left on the target
    eventually(
        "the late answer to be removed from the target",
        OBSERVE,
        || {
            let left = target.answers_left();
            left.is_empty()
                .then_some(())
                .ok_or_else(|| format!("answers still on the target: {left:?}"))
        },
    )
    .await;
    session.follower.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Every tool call is reported before it runs (`#target-parity` 4/7)
// ---------------------------------------------------------------------------------------------

/// What Claude Code hands its `PreToolUse` hook before it runs a shell command.
const PRE_TOOL_USE_PAYLOAD: &str = r#"{"hook_event_name":"PreToolUse","session_id":"c-41","tool_name":"Bash","tool_use_id":"toolu_01","tool_input":{"command":"pwd && ls"}}"#;

/// Reproduces gap 2 of the 2026-11-21 Coder probe: an agent's tool call never shows as running,
/// because no `PreToolUse` hook fires for an ordinary tool. A target's settings install none at
/// all (the only `PreToolUse` entry, matched to `AskUserQuestion`, waits for an answer and is left
/// out, `claude_hooks.rs:181`), and a local session's covers that one tool alone
/// (`claude_hooks.rs:90`), so every activity row is written by `PostToolUse` with
/// `started_unix_ms: 0`.
#[tokio::test]
async fn a_tool_call_of_an_agent_on_a_target_is_reported_before_it_runs() {
    // Given a target session whose hooks are installed in its worktree on the target
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let hook = target
        .installed_hook_for_tool("PreToolUse", "Bash")
        .expect("the hooks file on the target installs a PreToolUse hook for Bash");
    // And an agent there that is about to run a shell command as it starts
    let agent = an_agent_that_fires_as_it_starts_with(&target, &hook, PRE_TOOL_USE_PAYLOAD);

    // When
    let session = daemon.launches(&agent, &target, &hooks).await;
    let delivered = daemon.hooks_once_delivered("PreToolUse").await;

    // Then the daemon is told which call is starting, under the agent's own id for it
    let started = delivered
        .iter()
        .find(|(event, _)| event == "PreToolUse")
        .map(|(_, payload)| (payload["tool_name"].clone(), payload["tool_use_id"].clone()));
    assert_eq!(
        started,
        Some((serde_json::json!("Bash"), serde_json::json!("toolu_01")))
    );
    session.follower.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Answering a target agent's question (`#target-parity` 6/7)
// ---------------------------------------------------------------------------------------------

/// The question an agent on the target asks with `AskUserQuestion`.
fn the_question() -> serde_json::Value {
    serde_json::json!({"questions": [{
        "question": "Which language should the service use?",
        "header": "Language",
        "multiSelect": false,
        "options": [{"label": "Rust", "description": "Memory safe"}, {"label": "Go", "description": "Simple"}]
    }]})
}

/// An agent on `target` that asks a question as it starts, through the waiting `PreToolUse` hook
/// the session installed there for `AskUserQuestion`.
fn an_agent_asking_a_question(target: &ATarget) -> AStubAgent {
    let hook = target
        .installed_hook_for_tool("PreToolUse", "AskUserQuestion")
        .expect("the hooks file on the target installs a PreToolUse hook for AskUserQuestion");
    let payload = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "c-41",
        "tool_name": "AskUserQuestion",
        "tool_input": the_question(),
    });
    an_agent_that_fires_as_it_starts_with(target, &hook, &payload.to_string())
}

#[tokio::test]
async fn a_question_of_an_agent_on_a_target_is_held_for_its_orchestrators_answer() {
    // Given a target session whose orchestrator is there
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let orchestrator = AnOrchestrator::choosing(0, "Rust");
    let agent = an_agent_asking_a_question(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, Arc::clone(&orchestrator))
        .await;
    agent.hook_output_once_it_ends().await;

    // Then the daemon recorded the question and held for its answer
    assert_eq!(orchestrator.held_for(), vec![REQUEST_ID.to_string()]);
    session.follower.stop().await;
}

#[tokio::test]
async fn an_orchestrators_answer_to_a_target_agents_question_is_printed_by_its_hook() {
    // Given a target session whose orchestrator chooses "Rust"
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let agent = an_agent_asking_a_question(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, AnOrchestrator::choosing(0, "Rust"))
        .await;
    let printed = agent.hook_output_once_it_ends().await;

    // Then the hook hands the answer to Claude in the call's own input
    let mut answered = the_question();
    answered["answers"] = serde_json::json!({"Which language should the service use?": "Rust"});
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(printed.trim()).ok(),
        Some(serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": answered
            }
        }))
    );
    session.follower.stop().await;
}

#[tokio::test]
async fn a_question_nobody_answers_leaves_the_target_agent_to_ask_in_its_dialog() {
    // Given a target session with no orchestrator to answer
    let target = a_reachable_target();
    let daemon = a_daemon();
    let hooks = daemon.installs_the_sessions_hooks_on(&target).await;
    let agent = an_agent_asking_a_question(&target);

    // When
    let session = daemon
        .launches_answered_by(&agent, &target, &hooks, AnOrchestrator::absent())
        .await;

    // Then the hook ends at once with no decision, so Claude shows its own dialog
    assert_eq!(agent.hook_output_once_it_ends().await, "");
    session.follower.stop().await;
}
