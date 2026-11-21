//! Unit tests: what a claude-cli start launches its agent with, beyond the managed workflow's own.

use super::*;
use crate::project_home_launch::{project_home_wiring, spec_role_launch, ProjectHomeLaunch};
use crate::spec_launch::McpRegistration;
use std::path::PathBuf;

/// A claude-cli start's hooks, their `.claude/settings.local.json` written into the worktree.
fn install_claude_cli_hooks(
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    os_user: &str,
    session_id: &str,
    worktree_path: &Path,
) {
    let claude = tddy_coding_agent_specs::embedded_spec("claude").expect("claude.yaml is built in");
    let hooks = claude_cli_hooks(
        config,
        agent_tool_socket,
        &claude,
        "/usr/local/bin/tddy-tools",
    );
    let rendered = tddy_coding_agent_specs::slots::render(
        &claude,
        &tddy_coding_agent_specs::slots::SlotInputs {
            workdir: worktree_path.to_path_buf(),
            scratch_dir: worktree_path.to_path_buf(),
            mcp_server: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: None,
            hooks: Some(hooks.params(session_id, os_user)),
            home: None,
        },
    )
    .expect("the hooks slot renders");
    for (path, bytes) in &rendered.files {
        std::fs::create_dir_all(path.parent().expect("a hooks file has a parent"))
            .expect("the hooks directory is created");
        std::fs::write(path, bytes).expect("the hooks file is written");
    }
}

/// The loopback port a desktop daemon's GitHub sign-in returns on — and the only HTTP it answers.
const SIGN_IN_CALLBACK_PORT: u16 = 8899;

/// A daemon embedded in Tddy Desktop: it serves no HTTP RPC, and its one agent-tool socket is
/// bound (and held, as the runtime holds it).
struct AnEmbeddedHost {
    config: DaemonConfig,
    socket_path: PathBuf,
    _listener: std::os::unix::net::UnixListener,
}

impl AnEmbeddedHost {
    fn with_its_agent_tool_socket_bound() -> Self {
        let socket_path = tddy_daemon_kernel::next_embedded_socket_path();
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .expect("the agent-tool socket binds");
        let mut config = DaemonConfig::default();
        config.listen.web_port = Some(SIGN_IN_CALLBACK_PORT);
        Self {
            config,
            socket_path,
            _listener: listener,
        }
    }

    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone()
    }

    fn agent_tool_socket(&self) -> String {
        self.socket_path.to_string_lossy().into_owned()
    }
}

impl Drop for AnEmbeddedHost {
    fn drop(&mut self) {
        // Binding leaves the path behind; it lives under $TMPDIR, not the temporary data dir.
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// A binary daemon (`tddy-daemon`): it serves HTTP RPC, and serves each session's OS user an
/// agent-tool socket of their own under its data dir.
struct AServerHost {
    config: DaemonConfig,
    data_dir: tempfile::TempDir,
}

impl AServerHost {
    fn serving_its_users_agent_tool_socket() -> Self {
        Self {
            config: DaemonConfig::default(),
            data_dir: tempfile::tempdir().expect("a temporary data dir"),
        }
    }

    /// Where `ada`, the user the sessions here run as, is served.
    fn socket_path(&self) -> PathBuf {
        tddy_daemon_kernel::agent_tool_sockets::user_socket_path_for_user(
            self.data_dir.path(),
            ITS_OS_USER,
        )
        .expect("the user has a socket path")
    }

    fn agent_tool_socket(&self) -> String {
        self.socket_path().to_string_lossy().into_owned()
    }
}

/// The `--daemon-socket` argument a hook command carries for `socket`: the flag, then the path
/// shell-quoted — the command is a shell string.
fn the_daemon_socket_argument_for(socket: &str) -> String {
    format!(
        " --daemon-socket {} ",
        tddy_core::claude_hooks::shell_quote(socket)
    )
}

/// The shell command Claude Code runs for `event`, as installed in `worktree` for `session_id`.
fn the_installed_hook_command(worktree: &Path, session_id: &str, event: &str) -> String {
    let file =
        tddy_core::claude_hooks::session_file::claude_session_hooks_file(worktree, session_id);
    let settings = std::fs::read_to_string(&file)
        .expect("the session's hooks settings were written into the worktree");
    let settings: serde_json::Value =
        serde_json::from_str(&settings).expect("the hooks settings are JSON");
    settings["hooks"][event][0]["hooks"][0]["command"]
        .as_str()
        .expect("the event has a hook command")
        .to_string()
}

/// On Tddy Desktop the claude-cli hooks are pointed at `http://127.0.0.1:{listen.web_port}`, which
/// there is only the GitHub sign-in callback: every `ReportAgentActivity` and `ReportSessionStatus`
/// the hook POSTs is answered with an empty 404 that `tddy-tools session-hook` swallows. The session
/// never records an `agent-activity.jsonl`, so its Activity pane never appears and its status never
/// moves. The embedded daemon's agent-tool socket is the channel a co-located process has to it.
#[test]
fn on_an_embedded_host_the_tool_use_hook_reports_over_the_agent_tool_socket() {
    // Given a desktop daemon, whose agent-tool socket is bound
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    let worktree = tempfile::tempdir().expect("a temporary worktree");

    // When a claude-cli session's hooks are installed into its worktree
    install_claude_cli_hooks(
        &host.config,
        &host.socket_path(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reaches the daemon over that socket
    let command = the_installed_hook_command(
        worktree.path(),
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        "PostToolUse",
    );
    let expected = the_daemon_socket_argument_for(&host.agent_tool_socket());
    assert!(
        command.contains(&expected),
        "the PostToolUse hook must report over the agent-tool socket ({expected:?}), got: {command}"
    );
}

/// A server `tddy-daemon` serves each session's OS user an agent-tool socket of their own
/// (`#agent-tools` 26/28), so its hooks report over the user's socket, as a desktop's do over its one.
#[test]
fn on_a_server_host_the_tool_use_hook_reports_over_its_users_agent_tool_socket() {
    // Given a binary daemon, serving its user's agent-tool socket
    let host = AServerHost::serving_its_users_agent_tool_socket();
    let worktree = tempfile::tempdir().expect("a temporary worktree");

    // When a claude-cli session's hooks are installed into its worktree
    install_claude_cli_hooks(
        &host.config,
        &host.socket_path(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reaches the daemon over that user's socket
    let command = the_installed_hook_command(
        worktree.path(),
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        "PostToolUse",
    );
    let expected = the_daemon_socket_argument_for(&host.agent_tool_socket());
    assert!(
        command.contains(&expected),
        "a server host's PostToolUse hook must report over its user's socket ({expected:?}), got: {command}"
    );
}

const A_MAIN: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
const A_MINTED_TOKEN: &str = "minted-for-the-agent";

/// The flags and environment a main's launch gives its agent, as the `claude` spec's slots render
/// them.
struct RoleLaunch {
    extra_args: Vec<String>,
    env: Vec<(String, String)>,
}

/// A main's launch on `config` / `agent_tool_socket`, written into a fresh session dir.
struct AMainLaunch {
    session_dir: tempfile::TempDir,
    launch: RoleLaunch,
}

impl AMainLaunch {
    fn on(config: &DaemonConfig, agent_tool_socket: &Path) -> Self {
        let session_dir = tempfile::tempdir().expect("a temporary session dir");
        let wiring = project_home_wiring(ProjectHomeLaunch {
            config,
            agent_tool_socket,
            session_dir: session_dir.path(),
            session_id: A_MAIN,
            agent_session_token: A_MINTED_TOKEN,
            tddy_tools_path: Path::new("/opt/tddy/bin/tddy-tools"),
        })
        .expect("a main's launch is built");
        let (prompt_args, toolset) = spec_role_launch(wiring.main.as_ref());
        let tools_dir = tempfile::tempdir().expect("an empty tools dir");
        let agents = crate::spec_launch::HostCodingAgents::new(
            tddy_coding_agent_specs::load_agent_registry(tools_dir.path())
                .expect("the built-in coding agents load"),
            std::sync::Arc::new(tddy_actions::tool::ProcessToolHost),
        );
        let spec = agents
            .agents()
            .for_session_type("claude-cli")
            .expect("a built-in spec");
        let (mcp_server, scratch_dir) = match toolset.mcp {
            Some(McpRegistration { server, dir }) => (Some(server), dir),
            None => (None, session_dir.path().to_path_buf()),
        };
        let mut rendered = tddy_coding_agent_specs::slots::render(
            spec,
            &tddy_coding_agent_specs::slots::SlotInputs {
                workdir: session_dir.path().to_path_buf(),
                scratch_dir,
                mcp_server,
                allowed_tools: toolset.allowed_tools,
                denied_tools: toolset.denied_tools,
                permission_prompt_tool: None,
                preamble: None,
                append_system_prompt_file: None,
                hooks: None,
                home: None,
            },
        )
        .expect("the main's slots render");
        for (path, bytes) in &rendered.files {
            std::fs::write(path, bytes).expect("the slot's file is written");
        }
        rendered.argv_extra.extend(prompt_args);
        Self {
            session_dir,
            launch: RoleLaunch {
                extra_args: rendered.argv_extra,
                env: wiring.env,
            },
        }
    }

    fn env(&self, key: &str) -> Option<&str> {
        self.launch
            .env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn values_of(&self, flag: &str) -> Vec<&str> {
        self.launch
            .extra_args
            .windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
            .collect()
    }

    fn has_flag(&self, flag: &str) -> bool {
        self.launch.extra_args.iter().any(|arg| arg == flag)
    }
}

#[test]
fn on_an_embedded_host_a_main_reaches_the_daemon_over_its_agent_tool_socket() {
    // Given a desktop daemon, whose agent-tool socket is bound
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then its tools are pointed at that socket, which `tddy-tools` prefers over any URL
    assert_eq!(
        main.env("TDDY_REMOTE_DAEMON_SOCKET"),
        Some(host.agent_tool_socket().as_str())
    );
}

#[test]
fn on_a_server_host_a_main_reaches_the_daemon_over_its_users_agent_tool_socket() {
    // Given a binary daemon, serving its user's agent-tool socket
    let host = AServerHost::serving_its_users_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then its tools are pointed at that user's socket
    assert_eq!(
        main.env("TDDY_REMOTE_DAEMON_SOCKET"),
        Some(host.agent_tool_socket().as_str())
    );
}

#[test]
fn a_mains_tools_carry_its_own_session_id_and_the_minted_token() {
    // Given a binary daemon
    let host = AServerHost::serving_its_users_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then every tool call names the main itself, under the credential minted for it
    assert_eq!(main.env("TDDY_REMOTE_SESSION_ID"), Some(A_MAIN));
    assert_eq!(main.env("TDDY_REMOTE_SESSION_TOKEN"), Some(A_MINTED_TOKEN));
}

#[test]
fn a_main_loads_only_the_tddy_tools_mcp_server_from_its_session_dir() {
    // Given a binary daemon
    let host = AServerHost::serving_its_users_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then the one MCP config it loads is in its session dir, and no other config is loaded
    let config_path = main.session_dir.path().join("claude-mcp-config.json");
    assert_eq!(
        main.values_of("--mcp-config"),
        vec![config_path.to_str().unwrap()]
    );
    assert!(main.has_flag("--strict-mcp-config"));
    // And that config runs the given tddy-tools as an MCP server
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).expect("the config exists"))
            .expect("the config is JSON");
    assert_eq!(
        config["mcpServers"]["tddy-tools"]["command"],
        "/opt/tddy/bin/tddy-tools"
    );
    assert_eq!(
        config["mcpServers"]["tddy-tools"]["args"],
        serde_json::json!(["--mcp"])
    );
}

#[test]
fn a_main_is_pre_approved_its_tool_set_and_denied_every_native_filesystem_tool() {
    // Given a binary daemon
    let host = AServerHost::serving_its_users_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then it is pre-approved exactly the project-home tool set
    assert_eq!(
        main.values_of("--allowedTools"),
        tddy_tool_engine::read_only::project_home_allowed_tools()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    // And every native filesystem and shell tool is withdrawn outright
    assert_eq!(
        main.values_of("--disallowedTools"),
        tddy_session_split::split_session::NATIVE_FILESYSTEM_TOOLS.to_vec()
    );
}

#[test]
fn a_mains_role_prompt_is_written_to_its_session_dir_and_appended() {
    // Given a binary daemon
    let host = AServerHost::serving_its_users_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, &host.socket_path());

    // Then one prompt file, inside its session dir, is appended to its system prompt
    let prompt_files = main.values_of("--append-system-prompt-file");
    assert_eq!(prompt_files.len(), 1);
    let prompt_file = Path::new(prompt_files[0]);
    assert_eq!(prompt_file.parent(), Some(main.session_dir.path()));
    // And it tells the agent what it is and how it reaches the project's sessions
    let prompt = std::fs::read_to_string(prompt_file).expect("the prompt file exists");
    assert!(prompt.contains("main"), "prompt: {prompt}");
    assert!(prompt.contains("no shell"), "prompt: {prompt}");
    assert!(prompt.contains("ListProjectSessions"), "prompt: {prompt}");
    assert!(prompt.contains("StartSession"), "prompt: {prompt}");
    assert!(prompt.contains("ReadSessionWorktree"), "prompt: {prompt}");
    assert!(prompt.contains("DiffSessionWorktree"), "prompt: {prompt}");
}

#[test]
fn a_role_is_on_disk_before_the_agent_is_spawned() {
    // Given a session directory a start has just created
    let dir = tempfile::tempdir().expect("a temporary session dir");

    // When the role is recorded ahead of the spawn
    record_role_of_session_type_before_spawn(
        dir.path(),
        "home-1",
        "p-1",
        "project-home",
        "cursor-cli",
        Path::new("/work/home"),
        true,
    )
    .expect("recorded");

    // Then the record names the role, the project and the worktree, with no pid yet
    let record = tddy_core::read_session_metadata(dir.path()).expect("the record is readable");
    assert_eq!(record.session_role.as_deref(), Some("project-home"));
    assert_eq!(record.project_id, "p-1");
    assert_eq!(record.repo_path.as_deref(), Some("/work/home"));
    assert_eq!(record.session_type.as_deref(), Some("cursor-cli"));
    assert_eq!(record.pid, None);
}

#[test]
fn an_ordinary_session_writes_nothing_before_the_spawn() {
    // Given a session directory a start has just created
    let dir = tempfile::tempdir().expect("a temporary session dir");

    // When a session with no role has its role recorded ahead of the spawn
    record_role_of_session_type_before_spawn(
        dir.path(),
        "s-1",
        "p-1",
        "",
        "claude-cli",
        Path::new("/work"),
        false,
    )
    .expect("nothing to record is not a failure");

    // Then no record exists yet: the spawn writes the first one, as it always did
    assert!(tddy_core::read_session_metadata(dir.path()).is_err());
}

// ---------------------------------------------------------------------------
// `tddy-tools notify`: the environment and the guidance a claude-cli agent is launched with
// (`#home-inbox` 1/8)
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use tddy_service::session_activity::notify_env;

const A_NOTIFYING_SESSION: &str = "01a19c2e-55d0-7a41-9b3e-4f1d2c3b4a59";
const ITS_OS_USER: &str = "ada";
const ITS_HOOK_TOKEN: &str = "the-token-its-hooks-present";

/// The notify wiring a launch produced, read the way the agent's process reads it.
struct TheNotifyWiring(AgentNotifyWiring);

impl TheNotifyWiring {
    /// The environment as a map: what the agent can look up, in no particular order.
    fn environment(&self) -> BTreeMap<&str, &str> {
        self.0
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// The text of the file `claude` appends to its system prompt.
    fn appended_system_prompt(&self) -> String {
        let file = self
            .0
            .append_system_prompt_file
            .as_ref()
            .expect("the launch appends a system-prompt file");
        std::fs::read_to_string(file).expect("the system-prompt file exists")
    }
}

/// The notify environment a session whose host serves it the agent-tool socket `socket` is given.
fn the_environment_reaching_the_socket(socket: &str) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (notify_env::SESSION_ID, A_NOTIFYING_SESSION.to_string()),
        (notify_env::OS_USER, ITS_OS_USER.to_string()),
        (notify_env::HOOK_TOKEN, ITS_HOOK_TOKEN.to_string()),
        (notify_env::DAEMON_SOCKET, socket.to_string()),
    ])
}

fn as_borrowed<'a>(expected: &'a BTreeMap<&'static str, String>) -> BTreeMap<&'a str, &'a str> {
    expected.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// A plain claude-cli start's notify wiring on `config` / `agent_tool_socket`, with no managed recipe.
fn a_started_sessions_notify_wiring(
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    session_dir: &Path,
) -> TheNotifyWiring {
    TheNotifyWiring(
        agent_notify_launch(AgentNotifyLaunch {
            config,
            agent_tool_socket,
            os_user: ITS_OS_USER,
            session_id: A_NOTIFYING_SESSION,
            hook_token: ITS_HOOK_TOKEN,
            session_dir,
            managed_prompt_file: None,
            back_channel_guidance: None,
        })
        .expect("a start's notify wiring is built"),
    )
}

#[test]
fn a_claude_cli_session_is_launched_with_the_notify_environment_and_the_guidance_fragment() {
    // Given a binary daemon, serving its user's agent-tool socket
    let host = AServerHost::serving_its_users_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");

    // When a claude-cli session is started there
    let wiring =
        a_started_sessions_notify_wiring(&host.config, &host.socket_path(), session_dir.path());

    // Then its agent's environment names its session, its owner, its hook token and the user's
    // agent-tool socket — exactly what `tddy-tools notify` reads
    let expected = the_environment_reaching_the_socket(&host.agent_tool_socket());
    assert_eq!(wiring.environment(), as_borrowed(&expected));
    // And its system prompt is appended the paragraph telling it when to notify
    assert!(
        wiring
            .appended_system_prompt()
            .contains(crate::notify_env::AGENT_NOTIFY_GUIDANCE.trim()),
        "the appended system prompt carries the notify guidance: {:?}",
        wiring.appended_system_prompt()
    );
}

#[test]
fn a_resumed_claude_cli_session_carries_the_notify_environment_again() {
    // Given a binary daemon, and a stopped claude-cli session whose record holds its hook token
    let host = AServerHost::serving_its_users_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");
    let recorded = tddy_core::SessionMetadata {
        hook_token: Some(ITS_HOOK_TOKEN.to_string()),
        ..service_util::starting_session_metadata(A_NOTIFYING_SESSION, "p-1", "claude-cli")
    };

    // When it is resumed
    let wiring = TheNotifyWiring(
        resumed_agent_notify_launch(
            &host.config,
            &host.socket_path(),
            ITS_OS_USER,
            session_dir.path(),
            &recorded,
            None,
            None,
        )
        .expect("a resume's notify wiring is built"),
    );

    // Then its new process is given the same notify environment its first one was
    let expected = the_environment_reaching_the_socket(&host.agent_tool_socket());
    assert_eq!(wiring.environment(), as_borrowed(&expected));
}

#[test]
fn an_embedded_host_exports_the_daemon_socket_not_the_url() {
    // Given a desktop daemon, which serves no HTTP RPC and has its agent-tool socket bound
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");

    // When a claude-cli session is started there
    let wiring =
        a_started_sessions_notify_wiring(&host.config, &host.socket_path(), session_dir.path());

    // Then its agent reaches the daemon over that socket, and is told of no URL that answers nothing
    let socket = host.agent_tool_socket();
    assert_eq!(
        wiring.environment(),
        BTreeMap::from([
            (notify_env::SESSION_ID, A_NOTIFYING_SESSION),
            (notify_env::OS_USER, ITS_OS_USER),
            (notify_env::HOOK_TOKEN, ITS_HOOK_TOKEN),
            (notify_env::DAEMON_SOCKET, socket.as_str()),
        ])
    );
}

#[test]
fn a_managed_sessions_recipe_prompt_follows_the_notify_guidance_in_the_one_appended_file() {
    // Given a binary daemon, and a managed recipe whose orchestration prompt is already written
    let host = AServerHost::serving_its_users_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");
    let recipe_prompt = session_dir.path().join("recipe-prompt.md");
    std::fs::write(&recipe_prompt, "You are driving the tdd recipe.").expect("the recipe prompt");

    // When the session is started — `claude` takes a single `--append-system-prompt-file`
    let wiring = TheNotifyWiring(
        agent_notify_launch(AgentNotifyLaunch {
            config: &host.config,
            agent_tool_socket: &host.socket_path(),
            os_user: ITS_OS_USER,
            session_id: A_NOTIFYING_SESSION,
            hook_token: ITS_HOOK_TOKEN,
            session_dir: session_dir.path(),
            managed_prompt_file: Some(&recipe_prompt),
            back_channel_guidance: None,
        })
        .expect("a start's notify wiring is built"),
    );

    // Then the one file it appends carries the guidance, then the recipe's prompt, both whole
    assert_eq!(
        wiring.appended_system_prompt(),
        format!(
            "{}\n\nYou are driving the tdd recipe.",
            crate::notify_env::AGENT_NOTIFY_GUIDANCE.trim_end()
        )
    );
}

// ---------------------------------------------------------------------------------------------
// `/clear` in the project-home session was not recorded (reported 2026-11-21).
//
// A claude-cli session's hooks name its session id, its hook token and the daemon's agent-tool
// socket. They were one worktree-wide file (`.claude/settings.local.json`) that a resume never
// rewrote, so two ordinary events left them naming the wrong thing:
//
// 1. The daemon restarted: the agent-tool socket is `tddy-agent-tools-<daemon pid>-<n>.sock`, so the
//    file named a socket nothing listens on, and `tddy-tools session-hook` failed quietly.
// 2. Another session started in the same worktree: the project-home worktree also hosts the
//    project's orchestrator sessions, so the file named THEIR session id and token.
//
// Either way no hook of the resumed session reached it — its tool calls went unrecorded, and the
// `SessionEnd{reason: clear}` / `SessionStart{source: clear}` pair never reached the rotation that
// appends the new conversation to `agent_conversations`, so the next resume bound the old one.
// Each session now has a hooks file of its own (`.tddy/hooks/<session_id>/`), rendered again on
// every resume.
// ---------------------------------------------------------------------------------------------

const THE_HOME_SESSION: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
const THE_HOME_HOOK_TOKEN: &str = "b45669f6-e4d8-4808-8787-58e465c6a475";
const AN_ORCHESTRATOR_SESSION: &str = "01a1f0a7-11cd-7753-bb33-d733709924ef";

/// A stopped claude-cli session recorded with its hook token, as the daemon resumes it.
fn a_stopped_claude_cli_session(session_id: &str, hook_token: &str) -> tddy_core::SessionMetadata {
    tddy_core::SessionMetadata {
        hook_token: Some(hook_token.to_string()),
        ..service_util::starting_session_metadata(session_id, "p-1", "claude-cli")
    }
}

/// What a resume of `recorded` leaves installed in `worktree`: its hooks are rendered with the
/// claude spec's slots and written, exactly as the start path writes them — when the resume
/// renders any at all.
fn resume_its_hooks_into(
    config: &DaemonConfig,
    agent_tool_socket: &Path,
    recorded: &tddy_core::SessionMetadata,
    worktree: &Path,
) {
    let claude = tddy_coding_agent_specs::embedded_spec("claude").expect("claude.yaml is built in");
    let hooks = resumed_hooks(
        &claude,
        config,
        agent_tool_socket,
        "/usr/local/bin/tddy-tools",
        recorded,
    )
    .expect("a resume's hooks are decided");
    let rendered = tddy_coding_agent_specs::slots::render(
        &claude,
        &tddy_coding_agent_specs::slots::SlotInputs {
            workdir: worktree.to_path_buf(),
            scratch_dir: worktree.to_path_buf(),
            mcp_server: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: None,
            hooks: hooks
                .as_ref()
                .map(|hooks| hooks.params(&recorded.session_id, ITS_OS_USER)),
            home: None,
        },
    )
    .expect("the slots render");
    for (path, bytes) in &rendered.files {
        std::fs::create_dir_all(path.parent().expect("a hooks file has a parent"))
            .expect("the hooks directory is created");
        std::fs::write(path, bytes).expect("the hooks file is written");
    }
}

/// The hook events that carry a `/clear` (`SessionEnd`, `SessionStart`) and a tool call
/// (`PostToolUse`): each must report the resumed session, with its token, over the live socket.
fn assert_every_clear_and_tool_hook_reports(worktree: &Path, session_id: &str, socket: &str) {
    for event in ["SessionEnd", "SessionStart", "PostToolUse"] {
        let command = the_installed_hook_command(worktree, session_id, event);
        let session = format!(" --session {session_id} ");
        let token = format!(" --hook-token {THE_HOME_HOOK_TOKEN} ");
        let over = the_daemon_socket_argument_for(socket);
        assert!(
            command.contains(&session) && command.contains(&token) && command.contains(&over),
            "after the resume, the {event} hook must report session {session_id} with its own \
             token over the live agent-tool socket {socket:?}; the worktree still holds: {command}"
        );
    }
}

#[test]
fn a_resume_after_a_daemon_restart_rewires_the_hooks_to_the_live_agent_tool_socket() {
    // Given the home session's hooks, installed by a daemon that has since exited
    let worktree = tempfile::tempdir().expect("a temporary worktree");
    let gone = tempfile::tempdir().expect("the exited daemon's temp dir");
    let exited_socket = gone.path().join("tddy-agent-tools-46362-0.sock");
    install_claude_cli_hooks(
        &DaemonConfig::default(),
        &exited_socket,
        ITS_OS_USER,
        THE_HOME_SESSION,
        worktree.path(),
    );
    // (the start's token is fresh; the resume reads the one the record holds)
    let recorded = a_stopped_claude_cli_session(THE_HOME_SESSION, THE_HOME_HOOK_TOKEN);

    // When the restarted daemon, on a socket of its own, resumes the session
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    resume_its_hooks_into(
        &host.config,
        &host.socket_path(),
        &recorded,
        worktree.path(),
    );

    // Then its `/clear` and tool-call hooks reach this daemon
    assert_every_clear_and_tool_hook_reports(
        worktree.path(),
        THE_HOME_SESSION,
        &host.agent_tool_socket(),
    );
}

#[test]
fn a_session_started_in_the_same_worktree_leaves_a_resumed_sessions_hooks_its_own() {
    // Given one daemon, and the project-home worktree where an orchestrator session was started
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    let worktree = tempfile::tempdir().expect("the project-home worktree");
    install_claude_cli_hooks(
        &host.config,
        &host.socket_path(),
        ITS_OS_USER,
        AN_ORCHESTRATOR_SESSION,
        worktree.path(),
    );
    let recorded = a_stopped_claude_cli_session(THE_HOME_SESSION, THE_HOME_HOOK_TOKEN);

    // When the home session is resumed in that worktree
    resume_its_hooks_into(
        &host.config,
        &host.socket_path(),
        &recorded,
        worktree.path(),
    );

    // Then its `/clear` and tool-call hooks report the home session…
    assert_every_clear_and_tool_hook_reports(
        worktree.path(),
        THE_HOME_SESSION,
        &host.agent_tool_socket(),
    );
    // …and the orchestrator's still report the orchestrator
    let orchestrators =
        the_installed_hook_command(worktree.path(), AN_ORCHESTRATOR_SESSION, "Stop");
    assert!(
        orchestrators.contains(&format!(" --session {AN_ORCHESTRATOR_SESSION} ")),
        "the orchestrator's hooks must stay its own: {orchestrators}"
    );
}
