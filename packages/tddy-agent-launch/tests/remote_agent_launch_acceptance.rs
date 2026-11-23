//! Acceptance: a session's agent is launched on its target through the remote-shell port.
//!
//! The port is the real `SshShell` behind a stand-in `ssh` executable: a script that stands for
//! both the OpenSSH client and the host it reaches, running the remote command on this machine as
//! a child of its own — so, as over a real connection, the terminal's process is the client and
//! the agent is another process. A temp directory stands for the target's filesystem. The agent is
//! a stub script that records where and how it was started. Nothing opens a connection.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-ssh-agent-launch.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once};
use std::time::Duration;
use tddy_testing_commons::remote_target::write_executable;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, TemplateParams, ToolDef};
use tddy_agent_launch::remote_agent_launch::{
    launch_on_target, RemoteLaunch, RemoteLaunchError, RemoteLaunched,
};
use tddy_agent_launch::spec_launch::{HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner};
use tddy_remote_shell::SshShell;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e10";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";

/// How long the agent must stay up for a launch to count as started.
const STARTUP_WINDOW: Duration = Duration::from_millis(400);
/// How long the agent may take to record its pid on the target: generous, so no test but the one
/// that bounds it ever meets it.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// A safety net for a stub to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// The provider key the agent's spec declares in `pass_env`, and the daemon holds.
const DECLARED_KEY: &str = "ACME_API_KEY";
/// A provider key the daemon holds and the spec does not declare.
const UNDECLARED_KEY: &str = "OTHER_VENDOR_API_KEY";
/// A variable of the daemon's own.
const DAEMON_ONLY: &str = "TDDY_REMOTE_LAUNCH_PROBE";
/// A variable the target's login sets for every command it runs.
const LOGIN_ONLY: &str = "REMOTE_LOGIN_ONLY";
/// A locale only the daemon's host sets: no agent on a target may see it.
const DAEMON_LANG: &str = "daemon-host.UTF-8";
/// What the agent's spec sets in `run.env`.
const SPEC_SET: &str = "ACME_RUN_MODE";
/// What the launch sets on purpose.
const LAUNCH_SET: &str = "ROUTE_BASE_URL";

/// What the agent prints on its terminal once it is up.
const AGENT_BANNER: &str = "ACME READY on the target";
/// The system prompt the session appends for its agent, as the operator-notify wiring writes it.
const APPENDED_SYSTEM_PROMPT: &str =
    "Tell your operator with `tddy-tools notify` when you are blocked.";
/// The flag the agent is given its appended system prompt with.
const APPEND_SYSTEM_PROMPT_FLAG: &str = "--append-system-prompt-file";

/// What an agent that cannot start prints before it exits.
const AGENT_COMPLAINT: &str = "acme: not logged in";
const AGENT_FAILURE_STATUS: i32 = 3;
/// What OpenSSH says when it cannot reach its target.
const SSH_COMPLAINT: &str = "ssh: Could not resolve hostname buildbox: Name or service not known";

fn a_daemon_holding_two_provider_keys_and_a_variable_of_its_own() {
    static PLANTED: Once = Once::new();
    PLANTED.call_once(|| {
        std::env::set_var(DECLARED_KEY, "sk-acme");
        std::env::set_var(UNDECLARED_KEY, "sk-other");
        std::env::set_var(DAEMON_ONLY, "/etc/tddy/probe");
    });
}

/// A daemon host whose environment holds a locale and a `TMPDIR` of its own: both are in the
/// agent's base set, so they reach an agent that runs on the daemon's host, and must not reach one
/// on a target. Returns the daemon's `TMPDIR`.
fn a_daemon_host_with_its_own_locale_and_tmpdir() -> PathBuf {
    static PLANTED: Once = Once::new();
    let tmpdir = std::env::temp_dir().join("tddy-daemon-host-tmp");
    PLANTED.call_once(|| {
        std::fs::create_dir_all(&tmpdir).expect("the daemon host's TMPDIR exists");
        std::env::set_var("LANG", DAEMON_LANG);
        std::env::set_var("LC_ALL", DAEMON_LANG);
        std::env::set_var("TMPDIR", &tmpdir);
    });
    tmpdir
}

/// A machine the daemon reaches over ssh: a directory standing for its filesystem, holding the
/// session's worktree, and the stand-in `ssh` that reaches it.
struct ATarget {
    root: PathBuf,
    _dir: tempfile::TempDir,
}

/// One call of the stand-in `ssh`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AnSshCall {
    /// The directory the client was run in.
    cwd: PathBuf,
    /// Its arguments, joined by spaces.
    args: String,
}

/// The stand-in `ssh` of a target that runs what it is asked to: it records its working directory
/// and its arguments, then runs its last argument — the remote command — with `sh -c` as a child
/// whose terminal is its own, under a login environment holding [`LOGIN_ONLY`]. It hangs the
/// command up when it is itself hung up or terminated, and exits with the command's status.
/// `before` is run first.
fn a_target_that_runs_what_it_is_asked_to(before: &str) -> String {
    a_target_that_runs_what_it_is_asked_to_then(before, "")
}

/// [`a_target_that_runs_what_it_is_asked_to`], running `after` once the command has ended and
/// before the client exits with its status.
fn a_target_that_runs_what_it_is_asked_to_then(before: &str, after: &str) -> String {
    format!(
        r#"{before}for command; do :; done
printf '%s\t%s\n' "$(/bin/pwd -P)" "$(printf '%s' "$*" | /usr/bin/tr '\n' ' ')" >> "$(dirname "$0")/calls"
{LOGIN_ONLY}=from-the-login
export {LOGIN_ONLY}
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
status=$?
{after}exit $status
"#
    )
}

/// A reachable target whose login shell is its own, as every target is that runs an agent: `$SHELL` there runs `-lc <command>` under a
/// `HOME`, a `PATH` and a `TMPDIR` inside the target's filesystem, none of them this host's, and
/// finds the agent in the target's own `opt/agents`, which this host's `PATH` does not hold.
fn a_target_with_a_login_of_its_own() -> ATarget {
    let target = ATarget::reached_through(&a_target_that_runs_what_it_is_asked_to(
        "SHELL=\"$(dirname \"$0\")/login-shell\"\nexport SHELL\n",
    ));
    target.has_a_login_of_its_own();
    target
}

/// A target whose terminal takes `connect` to run its command, as a transport that reaches its
/// target anew for every terminal does (a Coder workspace's takes ~10 s): only the interactive
/// client (`ssh -tt`) is slow, so the probes for the agent's pid answer at once.
fn a_target_whose_terminal_connects_after(connect: Duration) -> ATarget {
    let target = ATarget::reached_through(&a_target_that_runs_what_it_is_asked_to(&format!(
        "SHELL=\"$(dirname \"$0\")/login-shell\"\nexport SHELL\n\
         if [ \"$1\" = -tt ]; then /bin/sleep {}; fi\n",
        connect.as_secs_f64()
    )));
    target.has_a_login_of_its_own();
    target
}

/// A target reached as a Coder workspace is: every one-shot command (the probes for the agent's
/// pid and its liveness) takes `round_trip` to answer, and the session's terminal (`ssh -tt`)
/// reports its end `terminal_lag` after its command has ended, as a client still tearing its
/// connection down does.
fn a_target_with_a_slow_round_trip_and_a_late_terminal_end(
    round_trip: Duration,
    terminal_lag: Duration,
) -> ATarget {
    let target = ATarget::reached_through(&a_target_that_runs_what_it_is_asked_to_then(
        &format!(
            "SHELL=\"$(dirname \"$0\")/login-shell\"\nexport SHELL\n\
             if [ \"$1\" != -tt ]; then /bin/sleep {}; fi\n",
            round_trip.as_secs_f64()
        ),
        &format!(
            "if [ \"$1\" = -tt ]; then /bin/sleep {}; fi\n",
            terminal_lag.as_secs_f64()
        ),
    ));
    target.has_a_login_of_its_own();
    target
}

/// A target whose liveness probes (`kill -0 <pid>`) never return: they sleep for `hang`, while
/// every other one-shot command answers at once and the session's terminal stays up — a transport
/// that carries the agent's pid back but has gone silent since.
fn a_target_whose_liveness_probes_never_answer(hang: Duration) -> ATarget {
    let target = ATarget::reached_through(&a_target_that_runs_what_it_is_asked_to(&format!(
        "SHELL=\"$(dirname \"$0\")/login-shell\"\nexport SHELL\n\
         case \"$*\" in *'kill -0'*) /bin/sleep {} ;; esac\n",
        hang.as_secs_f64()
    )));
    target.has_a_login_of_its_own();
    target
}

/// A target ssh cannot reach: the stand-in fails as OpenSSH does, [`SSH_COMPLAINT`] on stderr and
/// exit status 255, the remote command never run.
fn an_unreachable_target() -> ATarget {
    ATarget::reached_through(&format!("echo '{SSH_COMPLAINT}' >&2\nexit 255\n"))
}

impl ATarget {
    fn reached_through(ssh_body: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the target");
        let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
        std::fs::create_dir_all(root.join("bin")).expect("a directory for the stand-in ssh");
        write_executable(&root.join("bin/ssh"), &format!("#!/bin/sh\n{ssh_body}"));
        let target = Self { root, _dir: dir };
        std::fs::create_dir_all(target.worktree()).expect("the session's worktree on the target");
        target
    }

    /// The target's own login: its `HOME`, `PATH` and `TMPDIR` directories, and the login shell
    /// that sets them.
    fn has_a_login_of_its_own(&self) {
        for dir in [self.home(), self.agents_dir(), self.tmpdir()] {
            std::fs::create_dir_all(dir).expect("a directory of the target's login");
        }
        write_executable(
            &self.root.join("bin/login-shell"),
            &format!(
                "#!/bin/sh\n[ \"$1\" = -lc ] || exit 64\n\
                 HOME='{}'\nPATH='{}:{}:/usr/bin:/bin'\nTMPDIR='{}'\nTMUX_TMPDIR='{}'\n\
                 export HOME PATH TMPDIR TMUX_TMPDIR\n\
                 exec /bin/sh -c \"$2\"\n",
                self.home().display(),
                self.agents_dir().display(),
                tmux_dir().display(),
                self.tmpdir().display(),
                self.tmpdir().display()
            ),
        );
    }

    /// Whether the target's tmux — on its private socket under its `TMPDIR` — holds the tmux
    /// session `name`.
    fn has_tmux_session(&self, name: &str) -> bool {
        self.tmux(&["has-session", "-t", &format!("={name}")])
            .status
            .success()
    }

    /// The target's tmux, run on its private socket.
    fn tmux(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new(tmux_dir().join("tmux"))
            .args(args)
            .env("TMUX_TMPDIR", self.tmpdir())
            .env_remove("TMUX")
            .output()
            .expect("tmux runs")
    }

    /// The remote shell on the target, through the stand-in.
    fn shell(&self) -> Arc<SshShell> {
        Arc::new(SshShell::through(self.root.join("bin/ssh"), TARGET_ALIAS))
    }

    /// The session's worktree on the target.
    fn worktree(&self) -> PathBuf {
        self.root.join("home/dev/repo/.worktrees").join(SESSION_ID)
    }

    /// The `HOME` of the target's login.
    fn home(&self) -> PathBuf {
        self.root.join("home/dev")
    }

    /// Where the target's login finds its agents: the first directory of its `PATH`.
    fn agents_dir(&self) -> PathBuf {
        self.root.join("opt/agents")
    }

    /// The `TMPDIR` of the target's login.
    fn tmpdir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Which machine holds `path`: the target when it is inside the target's filesystem, this
    /// host (naming the path) otherwise.
    fn machine_holding(&self, path: &Path) -> String {
        if path.starts_with(&self.root) {
            "the target".to_string()
        } else {
            format!("the daemon's host, at {}", path.display())
        }
    }

    /// Where the session's pointer lives on the target.
    fn pointer_file(&self) -> PathBuf {
        self.worktree().join(".tddy/session.json")
    }

    /// Where the launch has the session's agent record its pid on the target.
    fn agent_pid_file(&self) -> PathBuf {
        self.worktree()
            .join(tddy_agent_launch::remote_agent_launch::remote_agent_pid_file(SESSION_ID))
    }

    /// A pointer placed in the session's worktree on the target, owned by this daemon — as the
    /// start places it before it launches the agent.
    fn a_placed_pointer(&self) -> SessionPointer {
        let pointer = SessionPointer {
            socket: None,
            repo_dir: self.worktree(),
            session_dir: None,
            remote: BTreeMap::new(),
            notify: BTreeMap::from([("TDDY_SESSION_ID".to_string(), SESSION_ID.to_string())]),
            owner: Some(SessionPointerOwner::this_process()),
        };
        let file = self.pointer_file();
        std::fs::create_dir_all(file.parent().expect("the pointer has a directory"))
            .expect("the pointer's directory on the target");
        std::fs::write(
            &file,
            serde_json::to_vec_pretty(&pointer).expect("a pointer encodes"),
        )
        .expect("the pointer is placed on the target");
        pointer
    }

    /// The pointer as the target holds it now.
    fn pointer(&self) -> SessionPointer {
        serde_json::from_slice(
            &std::fs::read(self.pointer_file()).expect("the pointer is on the target"),
        )
        .expect("the pointer on the target is a session pointer")
    }

    /// Every call of the stand-in so far whose arguments hold `needle`.
    fn calls_carrying(&self, needle: &str) -> Vec<AnSshCall> {
        std::fs::read_to_string(self.root.join("bin/calls"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .filter(|(_, args)| args.contains(needle))
            .map(|(cwd, args)| AnSshCall {
                cwd: PathBuf::from(cwd),
                args: args.to_string(),
            })
            .collect()
    }
}

impl Drop for ATarget {
    /// The target's tmux server, if a test started one on its private socket, goes with it.
    fn drop(&mut self) {
        if self.tmpdir().exists() {
            let _ = self.tmux(&["kill-server"]);
        }
    }
}

/// The directory holding this host's `tmux`, which the target's login finds on its `PATH`: the
/// stand-in target runs on this host.
fn tmux_dir() -> PathBuf {
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", "command -v tmux"])
        .output()
        .expect("`command -v tmux` runs");
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    path.parent()
        .unwrap_or_else(|| panic!("tmux is installed on this host: {path:?}"))
        .to_path_buf()
}

/// The name this host gives itself, as `hostname` prints it: the stand-in's target is this host.
fn this_hosts_name() -> String {
    let output = std::process::Command::new("hostname")
        .output()
        .expect("`hostname` runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A stub agent installed where the target finds it.
struct AStubAgent {
    dir: PathBuf,
    _dir: Option<tempfile::TempDir>,
}

/// What a stub agent recorded when it was started.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AnAgentReport {
    pid: u32,
    /// How the agent was found: its `$0`.
    program: PathBuf,
    cwd: PathBuf,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

/// An agent that records its pid, working directory, arguments and environment, prints
/// [`AGENT_BANNER`] on its terminal, then waits on it.
fn an_agent_that_stays_up() -> AStubAgent {
    AStubAgent::running(&the_body_of_an_agent_that_stays_up())
}

/// [`an_agent_that_stays_up`] as installed on a target's own `PATH`.
fn an_agent_that_stays_up_on(target: &ATarget) -> AStubAgent {
    AStubAgent::installed_on(target, &the_body_of_an_agent_that_stays_up())
}

fn the_body_of_an_agent_that_stays_up() -> String {
    format!(
        r#"printf '%s' "$$" > "$here/pid"
printf '%s' "$0" > "$here/program"
/bin/pwd -P > "$here/cwd"
for arg in "$@"; do printf '%s\n' "$arg"; done > "$here/args"
/usr/bin/env > "$here/env"
printf '{AGENT_BANNER}\n'
: > "$here/reported"
exec /bin/cat
"#
    )
}

/// An agent that cannot start: it prints [`AGENT_COMPLAINT`] and exits
/// [`AGENT_FAILURE_STATUS`] at once.
fn an_agent_that_exits_at_once_on(target: &ATarget) -> AStubAgent {
    AStubAgent::installed_on(
        target,
        &format!("printf '{AGENT_COMPLAINT}\\n'\nexit {AGENT_FAILURE_STATUS}\n"),
    )
}

impl AnAgentReport {
    /// The argument that follows `flag` on the agent's command line.
    fn value_of(&self, flag: &str) -> PathBuf {
        let at = self
            .args
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("the agent was given {flag}: {:?}", self.args));
        PathBuf::from(&self.args[at + 1])
    }
}

impl AStubAgent {
    fn running(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("a directory for the stub agent");
        let path = std::fs::canonicalize(dir.path()).expect("the stub's directory resolves");
        Self::written_to(path, Some(dir), body)
    }

    /// The agent `body` describes, installed in `dir`.
    fn written_to(dir: PathBuf, held: Option<tempfile::TempDir>, body: &str) -> Self {
        write_executable(
            &dir.join("acme"),
            &format!("#!/bin/sh\nhere='{}'\n{body}", dir.display()),
        );
        Self { dir, _dir: held }
    }

    /// This agent installed where `target`'s login finds it, by its bare name.
    fn installed_on(target: &ATarget, body: &str) -> Self {
        Self::written_to(target.agents_dir(), None, body)
    }

    fn program(&self) -> PathBuf {
        self.dir.join("acme")
    }

    /// Whether the agent was ever started.
    fn ran(&self) -> bool {
        self.dir.join("pid").exists()
    }

    /// What the agent recorded, once it has recorded all of it.
    async fn report(&self) -> AnAgentReport {
        eventually("the agent to report how it was started", OBSERVE, || {
            self.dir
                .join("reported")
                .exists()
                .then_some(())
                .ok_or_else(|| "the agent has not reported yet".to_string())
        })
        .await;
        let read = |name: &str| {
            std::fs::read_to_string(self.dir.join(name))
                .unwrap_or_else(|e| panic!("the agent recorded its {name}: {e}"))
        };
        AnAgentReport {
            pid: read("pid").trim().parse().expect("the agent's pid"),
            program: PathBuf::from(read("program")),
            cwd: PathBuf::from(read("cwd").trim_end()),
            args: read("args").lines().map(str::to_string).collect(),
            env: read("env")
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        }
    }
}

/// The `acme` coding agent as this host resolves it: its binary is `program`, its argv carries the
/// model, its spec sets [`SPEC_SET`] and passes [`DECLARED_KEY`].
fn the_acme_agent(program: &Path) -> HostAgent {
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
    - {{ splice: slots }}
  env: {{ {SPEC_SET}: on-target }}
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: {MODEL}, curated: [{MODEL}] }}
    pass_env: [{DECLARED_KEY}]
"#
    ))
    .expect("the tool file deserializes");
    HostAgent {
        spec: CodingAgentSpec::of(&def).expect("a valid coding agent"),
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

/// A daemon about to launch a session's agent: its terminals and the session's own directory.
struct ADaemon {
    manager: Arc<CliSessionManager>,
    session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

fn a_daemon() -> ADaemon {
    a_daemon_holding_two_provider_keys_and_a_variable_of_its_own();
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}

impl ADaemon {
    /// Launch `agent` on `target` for the session, rewriting `pointer`'s owner when one was placed.
    async fn launches(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
        pointer: Option<&SessionPointer>,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_resolved_at(&agent.program(), target, pointer)
            .await
    }

    /// [`Self::launches`] an agent this host resolved at `program`, wherever the target holds it.
    async fn launches_resolved_at(
        &self,
        program: &Path,
        target: &ATarget,
        pointer: Option<&SessionPointer>,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_with_slots(
            program,
            target,
            pointer,
            &RenderedSlots::default(),
            START_TIMEOUT,
        )
        .await
    }

    /// [`Self::launches`] `agent`, given only `start_timeout` to record its pid on the target.
    async fn launches_bounded_by(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
        start_timeout: Duration,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_with_slots(
            &agent.program(),
            target,
            None,
            &RenderedSlots::default(),
            start_timeout,
        )
        .await
    }

    /// [`Self::launches`] an agent told to append [`APPENDED_SYSTEM_PROMPT`], which the session
    /// wrote into its own directory on this host, as a start's operator-notify wiring writes it.
    async fn launches_appending_a_system_prompt(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        let prompt_file = self.session_dir.join("agent-system-prompt.md");
        std::fs::write(&prompt_file, APPENDED_SYSTEM_PROMPT)
            .expect("the system prompt is written into the session's directory");
        let slots = RenderedSlots {
            argv_extra: vec![
                APPEND_SYSTEM_PROMPT_FLAG.to_string(),
                prompt_file.to_string_lossy().into_owned(),
            ],
            ..RenderedSlots::default()
        };
        self.launches_with_slots(&agent.program(), target, None, &slots, START_TIMEOUT)
            .await
    }

    async fn launches_with_slots(
        &self,
        program: &Path,
        target: &ATarget,
        pointer: Option<&SessionPointer>,
        slots: &RenderedSlots,
        start_timeout: Duration,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_placed(program, target, pointer, slots, start_timeout, false)
            .await
    }

    /// [`Self::launches`] `agent` in a tmux session of its own on `target`.
    async fn launches_in_tmux(
        &self,
        agent: &AStubAgent,
        target: &ATarget,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        self.launches_placed(
            &agent.program(),
            target,
            None,
            &RenderedSlots::default(),
            START_TIMEOUT,
            true,
        )
        .await
    }

    async fn launches_placed(
        &self,
        program: &Path,
        target: &ATarget,
        pointer: Option<&SessionPointer>,
        slots: &RenderedSlots,
        start_timeout: Duration,
        tmux: bool,
    ) -> Result<RemoteLaunched, RemoteLaunchError> {
        launch_on_target(
            &the_acme_agent(program),
            &TemplateParams::new().with("model", MODEL),
            slots,
            RemoteLaunch {
                shell: target.shell(),
                worktree: &target.worktree(),
                session_dir: &self.session_dir,
                pointer,
                startup_window: STARTUP_WINDOW,
                start_timeout,
                clone_root: None,
                tmux,
            },
            HostPtySession {
                manager: &self.manager,
                session_id: SESSION_ID,
                session_type: SESSION_TYPE,
                model: MODEL,
                env: vec![(LAUNCH_SET.to_string(), "https://route.example".to_string())],
                os_user: None,
            },
            None,
        )
        .await
    }

    /// Whether the session has a terminal.
    async fn has_a_terminal(&self) -> bool {
        self.manager.get(SESSION_ID).await.is_some()
    }
}

#[tokio::test]
async fn the_agent_runs_on_the_target_in_the_sessions_worktree() {
    // Given a reachable target holding the session's worktree, and the agent installed there
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When
    let _launched = daemon
        .launches(&agent, &target, None)
        .await
        .expect("the agent is launched on the target");
    let report = agent.report().await;

    // Then the agent runs in the worktree on the target with the argv its spec renders
    assert_eq!(
        (report.cwd, report.args),
        (
            target.worktree(),
            vec!["--model".to_string(), MODEL.to_string()]
        )
    );

    // And the client that carries it ran once, for the target's alias, in the session's directory
    let launches = target.calls_carrying(&format!("> '.tddy/agents/{SESSION_ID}.pid'"));
    assert_eq!(
        launches
            .iter()
            .map(|call| (call.cwd.clone(), call.args.contains(TARGET_ALIAS)))
            .collect::<Vec<_>>(),
        vec![(daemon.session_dir.clone(), true)]
    );
}

#[tokio::test]
async fn the_agents_output_reaches_the_session_terminal() {
    // Given
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When
    let launched = daemon
        .launches(&agent, &target, None)
        .await
        .expect("the agent is launched on the target");

    // Then what the agent prints on the target is on the session's main terminal
    eventually("the agent's banner on the terminal", OBSERVE, || {
        let shown =
            String::from_utf8_lossy(launched.handle.capture.lock().unwrap().buffered_bytes())
                .into_owned();
        shown
            .contains(AGENT_BANNER)
            .then_some(())
            .ok_or(format!("the terminal shows: {shown}"))
    })
    .await;

    // And that terminal is the session's main one
    assert!(daemon.has_a_terminal().await);
}

#[tokio::test]
async fn the_agent_sees_only_its_specs_environment() {
    // Given a daemon holding a declared key, an undeclared key and a variable of its own, and a
    // target whose login sets a variable for every command
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When
    let _launched = daemon
        .launches(&agent, &target, None)
        .await
        .expect("the agent is launched on the target");
    let env = agent.report().await.env;

    // Then the agent has its spec's passed key, its spec's own variable and what the launch set
    let kept = [DECLARED_KEY, SPEC_SET, LAUNCH_SET].map(|name| env.get(name).map(String::as_str));
    assert_eq!(
        kept,
        [
            Some("sk-acme"),
            Some("on-target"),
            Some("https://route.example")
        ]
    );

    // And nothing else of the daemon's, and nothing of the target's login
    let leaked: Vec<&str> = [UNDECLARED_KEY, DAEMON_ONLY, LOGIN_ONLY]
        .into_iter()
        .filter(|name| env.contains_key(*name))
        .collect();
    assert_eq!(leaked, Vec::<&str>::new());
}

#[tokio::test]
async fn the_pointer_on_the_target_names_the_remote_agent_as_its_owner() {
    // Given a pointer the start placed on the target, owned by this daemon
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    let placed = target.a_placed_pointer();

    // When
    let launched = daemon
        .launches(&agent, &target, Some(&placed))
        .await
        .expect("the agent is launched on the target");
    let report = agent.report().await;

    // Then the launch answers the agent on the target as the owner, not the local client
    let the_remote_agent = SessionPointerOwner {
        pid: report.pid,
        host: this_hosts_name(),
    };
    assert_eq!(
        (launched.owner.clone(), launched.handle.pid == report.pid),
        (Some(the_remote_agent.clone()), false)
    );

    // And the pointer on the target names it, everything else as placed, the pid file consumed
    assert_eq!(
        (target.pointer(), target.agent_pid_file().exists()),
        (
            SessionPointer {
                owner: Some(the_remote_agent),
                ..placed
            },
            false
        )
    );
}

#[tokio::test]
async fn an_agent_that_exits_in_the_startup_window_fails_the_start_with_its_last_output() {
    // Given an agent on the target that exits as soon as it is started
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_exits_at_once_on(&target);
    let daemon = a_daemon();

    // When
    let failure = daemon
        .launches(&agent, &target, None)
        .await
        .err()
        .expect("a launch whose agent exits at once fails");

    // Then the failure carries the agent's exit status and what it printed last
    assert!(
        matches!(
            &failure,
            RemoteLaunchError::ExitedDuringStartup { status: Some(AGENT_FAILURE_STATUS), last_output }
                if last_output.contains(AGENT_COMPLAINT)
        ),
        "the launch failed with: {failure:?}"
    );

    // And the session is left without a terminal
    assert!(!daemon.has_a_terminal().await);
}

#[tokio::test]
async fn an_agent_whose_terminal_has_not_run_it_within_the_start_timeout_fails_the_start() {
    // Given a target whose terminal runs its command only after two seconds
    let target = a_target_whose_terminal_connects_after(Duration::from_secs(2));
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When the launch is given one second for the agent to record its pid there
    let failure = daemon
        .launches_bounded_by(&agent, &target, Duration::from_secs(1))
        .await
        .err()
        .expect("a launch whose agent is not started in time fails");

    // Then the failure names that bound, not one derived from the startup window
    assert_eq!(
        failure,
        RemoteLaunchError::AgentNeverStarted {
            after: Duration::from_secs(1)
        }
    );

    // And the session is left without a terminal
    assert!(!daemon.has_a_terminal().await);
}

/// Reproduces the `kill -0 <pid>` liveness probe recorded as `timed_out` after ~500 ms on the
/// Coder workspace `mantasi/remote-dev` (2026-11-21), where every one-shot call takes 6-9 s.
///
/// The probe in `exits_during_startup` is bounded by what the startup window has left
/// (`spawn_startup_grace_period_ms`, 500 ms by default), and a probe that does not answer leaves
/// the agent judged alive. Over a transport whose round trip is longer than the window, no probe
/// ever answers, so an agent that dies on the target counts as started as long as its terminal
/// is slow to report the end.
#[tokio::test]
async fn an_agent_that_dies_on_a_slow_target_fails_the_start_though_its_terminal_reports_late() {
    // Given a target whose one-shot commands take longer to answer than the startup window, and
    // whose terminal reports its end only well after the window has closed
    let target = a_target_with_a_slow_round_trip_and_a_late_terminal_end(
        STARTUP_WINDOW * 3,
        STARTUP_WINDOW * 10,
    );
    // And an agent that records its pid there, then exits at once
    let agent = an_agent_that_exits_at_once_on(&target);
    let daemon = a_daemon();

    // When
    let outcome = daemon.launches(&agent, &target, None).await;

    // Then the start fails: the agent's pid is dead on the target
    assert!(
        matches!(outcome, Err(RemoteLaunchError::ExitedDuringStartup { .. })),
        "the launch of an agent already dead on the target returned: {:?}",
        outcome.as_ref().map(|_| "a started session")
    );
}

/// The other half of the probe's judgement: on the same slow target, an agent that stays up is
/// started — a probe that has a round trip to answer finds it alive.
#[tokio::test]
async fn an_agent_that_stays_up_on_a_slow_target_starts() {
    // Given a target whose one-shot commands take longer to answer than the startup window
    let target = a_target_with_a_slow_round_trip_and_a_late_terminal_end(
        STARTUP_WINDOW * 3,
        STARTUP_WINDOW * 10,
    );
    // And an agent there that stays up
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When
    let outcome = daemon.launches(&agent, &target, None).await;

    // Then
    assert!(
        outcome.is_ok(),
        "the launch of an agent that stays up failed: {:?}",
        outcome.err()
    );
}

/// A hung transport is not a live agent: probes that never answer, with the terminal still up,
/// end the judgement at the start's own bound (the startup window plus the start timeout) with a
/// failure that names it — the start neither probes forever nor counts the agent alive.
#[tokio::test]
async fn an_agent_on_a_target_that_never_answers_fails_the_start_after_its_bound() {
    // Given a target whose liveness probes never return, and whose terminal stays up
    let target = a_target_whose_liveness_probes_never_answer(Duration::from_secs(60));
    // And an agent there that stays up
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When the launch is given a small start timeout
    let start_timeout = Duration::from_secs(1);
    let started = std::time::Instant::now();
    let failure = daemon
        .launches_bounded_by(&agent, &target, start_timeout)
        .await
        .err()
        .expect("a start whose probes never answer fails");

    // Then the failure names the bound: the startup window plus the start timeout
    assert_eq!(
        failure,
        RemoteLaunchError::AgentNeverAnswered {
            after: STARTUP_WINDOW + start_timeout
        }
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the start took {:?} to give up",
        started.elapsed()
    );

    // And the session is left without a terminal
    assert!(!daemon.has_a_terminal().await);
}

#[tokio::test]
async fn an_unreachable_target_fails_the_start_and_leaves_no_session() {
    // Given a target ssh cannot reach
    let target = an_unreachable_target();
    let agent = an_agent_that_stays_up();
    let daemon = a_daemon();

    // When
    let failure = daemon
        .launches(&agent, &target, None)
        .await
        .err()
        .expect("a launch on an unreachable target fails");

    // Then the failure says what the shell said
    assert!(
        failure.to_string().contains(SSH_COMPLAINT),
        "the launch failed with: {failure}"
    );

    // And nothing is left: no terminal, and the agent never ran
    assert_eq!((daemon.has_a_terminal().await, agent.ran()), (false, false));
}

#[tokio::test]
async fn a_target_runs_its_own_agent_with_its_own_home_path_and_tmpdir() {
    // Given a target whose login has a home, a PATH and a TMPDIR of its own and finds the agent
    // there by its bare name, and a daemon whose host resolves that agent somewhere else
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    let on_the_daemons_host = Path::new("/daemon-host/opt/acme/bin/acme");

    // When
    let _launched = daemon
        .launches_resolved_at(on_the_daemons_host, &target, None)
        .await
        .expect("the agent is launched on the target, which holds it");
    let report = agent.report().await;
    let seen = ["HOME", "PATH", "TMPDIR"].map(|name| report.env.get(name).cloned());

    // Then the agent that ran is the target's own, found by its bare name
    assert_eq!(report.program, target.agents_dir().join("acme"));

    // And it sees the target's home, PATH and TMPDIR, as the target's login shell expanded them
    assert_eq!(
        seen,
        [
            Some(target.home().display().to_string()),
            Some(format!("{}:/usr/bin:/bin", target.agents_dir().display())),
            Some(target.tmpdir().display().to_string()),
        ]
    );
}

#[tokio::test]
async fn the_daemon_hosts_environment_never_reaches_a_target_agent() {
    // Given a daemon host holding its own locale and TMPDIR, and a target with a login of its own
    let daemons_tmpdir = a_daemon_host_with_its_own_locale_and_tmpdir();
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    let daemons = [
        ("LANG", DAEMON_LANG.to_string()),
        ("LC_ALL", DAEMON_LANG.to_string()),
        ("TMPDIR", daemons_tmpdir.display().to_string()),
        ("HOME", std::env::var("HOME").expect("the daemon's HOME")),
        ("PATH", std::env::var("PATH").expect("the daemon's PATH")),
    ];

    // When
    let _launched = daemon
        .launches(&agent, &target, None)
        .await
        .expect("the agent is launched on the target");
    let env = agent.report().await.env;

    // Then none of the daemon host's values is in the agent's environment
    let leaked: Vec<&str> = daemons
        .iter()
        .filter(|(name, value)| env.get(*name) == Some(value))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(leaked, Vec::<&str>::new());
}

/// The appended system prompt is a file in the session's directory on the daemon's host. The
/// launch hands the agent on the target that host path, and writes the file at the same host path
/// through the target's shell, so on a target whose filesystem has no such directory (a Linux
/// Coder workspace given `/Users/<operator>/.tddy/sessions/<id>/…`) the start fails at
/// `mkdir -p '/Users/…'`.
#[tokio::test]
async fn the_agent_on_a_target_reads_its_appended_system_prompt_from_the_target() {
    // Given a target with the agent installed, and a daemon whose session appends a system prompt
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When
    let _launched = daemon
        .launches_appending_a_system_prompt(&agent, &target)
        .await
        .expect("the agent is launched on the target");
    let prompt_file = agent.report().await.value_of(APPEND_SYSTEM_PROMPT_FLAG);

    // Then the file the agent is told to read is on the target, holding the session's prompt
    assert_eq!(
        (
            target.machine_holding(&prompt_file),
            std::fs::read_to_string(&prompt_file).ok()
        ),
        (
            "the target".to_string(),
            Some(APPENDED_SYSTEM_PROMPT.to_string())
        )
    );
}

/// The tmux session the session's agent runs in on the target.
fn the_sessions_tmux_session() -> String {
    tddy_agent_launch::remote_agent_launch::target_tmux_session_name(SESSION_ID)
}

#[tokio::test]
async fn an_agent_run_in_tmux_runs_in_a_tmux_session_named_for_the_session_on_the_target() {
    // Given a reachable target with tmux, holding the session's worktree and the agent
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();

    // When the agent is launched in tmux
    let _launched = daemon
        .launches_in_tmux(&agent, &target)
        .await
        .expect("the agent is launched in tmux on the target");
    let report = agent.report().await;

    // Then it runs in the worktree, in the session's tmux session on the target, with no pid file
    assert_eq!(
        (
            report.cwd,
            target.has_tmux_session(&the_sessions_tmux_session()),
            target.agent_pid_file().exists()
        ),
        (target.worktree(), true, false)
    );
}

#[tokio::test]
async fn an_agent_run_in_tmux_outlives_its_dropped_connection() {
    // Given an agent running in tmux on the target
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    let _launched = daemon
        .launches_in_tmux(&agent, &target)
        .await
        .expect("the agent is launched in tmux on the target");
    agent.report().await;

    // When the session's connection to the target drops
    daemon.manager.stop_session(SESSION_ID).await;
    eventually_awaiting("the session's terminal to end", OBSERVE, || async {
        (!daemon.has_a_terminal().await)
            .then_some(())
            .ok_or_else(|| "the terminal is still registered".to_string())
    })
    .await;

    // Then the agent's tmux session is still running on the target
    assert!(target.has_tmux_session(&the_sessions_tmux_session()));
}

#[tokio::test]
async fn a_second_launch_in_tmux_re_attaches_to_the_running_agent() {
    // Given an agent running in tmux on the target whose connection dropped
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    daemon
        .launches_in_tmux(&agent, &target)
        .await
        .expect("the agent is launched in tmux on the target");
    let first = agent.report().await;
    daemon.manager.stop_session(SESSION_ID).await;

    // When the session is launched in tmux again
    let relaunched = daemon.launches_in_tmux(&agent, &target).await;

    // Then it attached to the same agent: started, and the agent never started a second time
    let again = agent.report().await;
    assert_eq!((relaunched.is_ok(), again.pid), (true, first.pid));
}

#[tokio::test]
async fn ending_a_tmux_run_kills_its_tmux_session_on_the_target() {
    // Given an agent running in tmux on the target
    let target = a_target_with_a_login_of_its_own();
    let agent = an_agent_that_stays_up_on(&target);
    let daemon = a_daemon();
    daemon
        .launches_in_tmux(&agent, &target)
        .await
        .expect("the agent is launched in tmux on the target");
    agent.report().await;
    let running_before = target.has_tmux_session(&the_sessions_tmux_session());

    // When the session's tmux run is ended
    let ended = tddy_agent_launch::remote_agent_launch::end_target_tmux_session(
        target.shell().as_ref(),
        SESSION_ID,
    )
    .await;

    // Then its tmux session, running until then, is gone from the target
    assert_eq!(
        (
            running_before,
            ended,
            target.has_tmux_session(&the_sessions_tmux_session())
        ),
        (true, Ok(()), false)
    );
}

#[tokio::test]
async fn ending_a_tmux_run_whose_session_is_gone_is_not_an_error() {
    // Given a target with tmux where the session's tmux session is not running
    let target = a_target_with_a_login_of_its_own();

    // When the session's tmux run is ended
    let ended = tddy_agent_launch::remote_agent_launch::end_target_tmux_session(
        target.shell().as_ref(),
        SESSION_ID,
    )
    .await;

    // Then
    assert_eq!(ended, Ok(()));
}
