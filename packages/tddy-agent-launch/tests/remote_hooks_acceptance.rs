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

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef};
use tddy_agent_launch::remote_agent_launch::{launch_on_target, RemoteLaunch, RemoteLaunched};
use tddy_agent_launch::remote_hook_follower::{
    create_remote_hook_log, remove_remote_hook_log, HookLine, HookSink, RemoteHookFollower,
};
use tddy_agent_launch::slot_files_over_shell::write_slot_files_through;
use tddy_agent_launch::spec_launch::{HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::{self, RenderedSlots, SlotInputs};
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::{
    activity_status_from_hook_with_tool, HookCommandParams, SessionActivityStatus, REMOTE_HOOK_LOG,
};
use tddy_remote_shell::{RemoteShellError, SshShell};
use tddy_testing_commons::wait::eventually;

/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e10";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
/// The session's hook token, as its `.session.yaml` holds it.
const HOOK_TOKEN: &str = "tok-7f3a9c";

/// How long the agent must stay up for a launch to count as started.
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// A safety net for a stub to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// What the agent hands its `SessionStart` hook.
const SESSION_START_PAYLOAD: &str =
    r#"{"hook_event_name":"SessionStart","session_id":"c-41","source":"startup"}"#;

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

/// A machine the daemon reaches over ssh: a directory standing for its filesystem, holding the
/// session's worktree, and the stand-in `ssh` that reaches it.
struct ATarget {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

/// A target that runs what it is asked to: the stand-in runs its last argument — the remote
/// command — with `sh -c` as a child of its own, its stdin passed on. It hangs the command up
/// when it is itself hung up or terminated, and exits with the command's status.
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
exit $?
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

    /// The command Claude Code would run for `event`, read from the settings file the launch
    /// installed in the worktree on the target.
    fn installed_hook(&self, event: &str) -> String {
        let settings: serde_json::Value = serde_json::from_slice(
            &std::fs::read(self.worktree().join(".claude/settings.local.json"))
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
    let path = target.agents_dir();
    std::fs::write(path.join("session-start-hook"), hook_command)
        .expect("the hook command is kept for the stub");
    write_executable(
        &path.join("acme"),
        &format!(
            r#"#!/bin/sh
here='{here}'
PATH=/usr/bin:/bin
export PATH
printf '%s' '{SESSION_START_PAYLOAD}' | /bin/sh -c "$(/bin/cat "$here/session-start-hook")"
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
}

/// The `acme` coding agent's tool file: its hooks are Claude Code's settings file.
fn the_acme_spec() -> CodingAgentSpec {
    let def: ToolDef = serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: acme
summary: a coding agent
binary:
  command: acme
deps: []
run:
  transport: pty
  argv:
    - "{{binary}}"
    - {{ if: model, then: ["--model", "{{model}}"] }}
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: {MODEL}, curated: [{MODEL}] }}
"#
    ))
    .expect("the tool file deserializes");
    CodingAgentSpec::of(&def).expect("a valid coding agent")
}

/// The `acme` agent as this host resolves it: its binary is `program`.
fn the_acme_agent(program: &Path) -> HostAgent {
    HostAgent {
        spec: the_acme_spec(),
        tool: ResolvedTool {
            id: "acme".to_string(),
            command: "acme".to_string(),
            binary: program.to_path_buf(),
            reads: Vec::new(),
            network: Default::default(),
        },
        deps: Vec::new(),
        env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
    }
}

/// The session's activity as the daemon keeps it: every status a delivered hook set, in order.
#[derive(Default)]
struct TheSessionsActivity {
    statuses: Mutex<Vec<SessionActivityStatus>>,
}

#[async_trait]
impl HookSink for TheSessionsActivity {
    async fn deliver(&self, line: HookLine) {
        let status = activity_status_from_hook_with_tool(
            &line.event,
            line.payload["notification_type"].as_str(),
            line.payload["tool_name"].as_str(),
        );
        self.statuses.lock().unwrap().extend(status);
    }
}

/// The hooks of a target session, installed: the slots the launch renders and where they log.
struct InstalledHooks {
    slots: RenderedSlots,
    log: String,
}

/// A target session that is running: its terminal and the follower of its hook log.
struct ARunningSession {
    _launched: RemoteLaunched,
    follower: RemoteHookFollower,
}

/// A daemon with one target session: its terminals, the session's own directory and its activity.
struct ADaemon {
    manager: Arc<CliSessionManager>,
    activity: Arc<TheSessionsActivity>,
    session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

fn a_daemon() -> ADaemon {
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        activity: Arc::default(),
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}

impl ADaemon {
    /// What a start does before it launches the agent: create the hook log on the target, render
    /// the agent's hooks to log there, and write the hooks file into the worktree on the target.
    async fn installs_the_sessions_hooks_on(&self, target: &ATarget) -> InstalledHooks {
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
                    answer_wait_secs: 120,
                    remote_log: Some(&log),
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
                clone_root: None,
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
