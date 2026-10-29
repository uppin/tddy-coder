//! Unit tests: what a claude-cli start launches its agent with, beyond the managed workflow's own.

use super::*;
use crate::project_home_launch::{project_home_launch, ProjectHomeLaunch, RoleLaunch};

fn the_accounts_git_pairs() -> Vec<(String, String)> {
    vec![
        ("GIT_AUTHOR_NAME".to_string(), "ada".to_string()),
        ("GIT_COMMITTER_NAME".to_string(), "ada".to_string()),
    ]
}

/// A start with no recipe and no index, so the only environment is the one under test.
async fn the_launch_environment_with(
    git_environment: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let progress = tddy_session_files::attachment_progress::AttachmentProgressSink::discarding();
    let registry = TaskRegistry::new();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (_, _, env_extra) = managed_claude_cli_launch(ManagedClaudeCliLaunch {
        tddy_data_dir: dir.path(),
        session_id: "s1",
        managed_recipe: &None,
        child_spawn_handler: None,
        conversation_spawn_handler: None,
        semantic_index: false,
        progress: &progress,
        task_registry: &registry,
        session_dir: dir.path(),
        worktree_path: dir.path(),
        tddy_tools_path: String::new(),
        git_environment,
        github_credential_handler: None,
    })
    .await
    .expect("a launch with nothing to prepare succeeds");
    env_extra
}

#[tokio::test]
async fn the_resolved_accounts_pairs_are_in_the_agents_environment() {
    // Given a start whose project resolved to an account
    // When the launch environment is built
    let env = the_launch_environment_with(the_accounts_git_pairs()).await;

    // Then the agent is launched with exactly those pairs
    assert_eq!(env, the_accounts_git_pairs());
}

#[tokio::test]
async fn a_start_that_resolved_nothing_adds_no_git_variables() {
    // Given a start whose project resolved to no account
    // When the launch environment is built
    let env = the_launch_environment_with(Vec::new()).await;

    // Then the agent inherits the checkout's identity: nothing is added
    assert!(env.is_empty());
}

/// The loopback port a desktop daemon's GitHub sign-in returns on — and the only HTTP it answers.
const SIGN_IN_CALLBACK_PORT: u16 = 8899;

/// A daemon embedded in Tddy Desktop: it serves no HTTP RPC, and its agent-tool socket is bound.
///
/// The socket is bound for real and held, because "is this host embedded" is answered by whether
/// something is listening on that socket.
struct AnEmbeddedHost {
    config: DaemonConfig,
    socket_path: PathBuf,
    _listener: std::os::unix::net::UnixListener,
    data_dir: tempfile::TempDir,
}

impl AnEmbeddedHost {
    fn with_its_agent_tool_socket_bound() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        let socket_path = tddy_daemon_kernel::agent_tool_socket_path(data_dir.path());
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .expect("the agent-tool socket binds");
        let mut config = DaemonConfig::default();
        config.listen.web_port = Some(SIGN_IN_CALLBACK_PORT);
        Self {
            config,
            socket_path,
            _listener: listener,
            data_dir,
        }
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
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

/// A binary daemon (`tddy-daemon`): it serves HTTP RPC and never binds an agent-tool socket.
struct ARegularHost {
    config: DaemonConfig,
    data_dir: tempfile::TempDir,
}

impl ARegularHost {
    fn without_an_agent_tool_socket() -> Self {
        Self {
            config: DaemonConfig::default(),
            data_dir: tempfile::tempdir().expect("a temporary data dir"),
        }
    }

    fn data_dir(&self) -> &Path {
        self.data_dir.path()
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

/// The shell command Claude Code runs for `event`, as installed in `worktree`.
fn the_installed_hook_command(worktree: &Path, event: &str) -> String {
    let settings = std::fs::read_to_string(worktree.join(".claude/settings.local.json"))
        .expect("the hooks settings were written into the worktree");
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
        host.data_dir(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reaches the daemon over that socket
    let command = the_installed_hook_command(worktree.path(), "PostToolUse");
    let expected = the_daemon_socket_argument_for(&host.agent_tool_socket());
    assert!(
        command.contains(&expected),
        "the PostToolUse hook must report over the agent-tool socket ({expected:?}), got: {command}"
    );
}

/// A regular `tddy-daemon` serves HTTP RPC on `daemon_url` and binds no agent-tool socket, so its
/// hooks must not be told of one: a `--daemon-socket` there would send every report to a path
/// nobody serves.
#[test]
fn on_a_regular_host_the_tool_use_hook_names_no_daemon_socket() {
    // Given a binary daemon, whose agent-tool socket is not bound
    let host = ARegularHost::without_an_agent_tool_socket();
    let worktree = tempfile::tempdir().expect("a temporary worktree");

    // When a claude-cli session's hooks are installed into its worktree
    install_claude_cli_hooks(
        &host.config,
        host.data_dir(),
        "ada",
        "01a14a71-f790-7083-8ad8-e4df3feed64e",
        worktree.path(),
    );

    // Then the tool-use hook reports over HTTP alone
    let command = the_installed_hook_command(worktree.path(), "PostToolUse");
    assert!(
        !command.contains("--daemon-socket"),
        "a regular host's PostToolUse hook must name no daemon socket, got: {command}"
    );
}

/// The values `--allowedTools` is given in `argv`, in order.
fn allowed_tools_in(argv: &[String]) -> Vec<&str> {
    argv.windows(2)
        .filter(|pair| pair[0] == "--allowedTools")
        .map(|pair| pair[1].as_str())
        .collect()
}

#[test]
fn a_project_home_spawn_is_pre_approved_exactly_the_project_home_tool_set() {
    // Given the argument a project-home claude-cli spawn adds to its command line
    let argv = role_allowed_tools_args(Some("project-home"));

    // Then each tool of the set is its own --allowedTools value, and nothing else is added
    let expected = tddy_tool_engine::read_only::project_home_allowed_tools();
    assert_eq!(
        allowed_tools_in(&argv),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(argv.len(), expected.len() * 2);
    assert!(!allowed_tools_in(&argv).contains(&"mcp__tddy-tools__Write"));
}

#[test]
fn an_ordinary_spawn_adds_no_allowed_tools() {
    // Given an ordinary session, and a role the daemon gives no tool set
    // Then neither adds an argument, so the spawn's argv is what it always was
    assert!(role_allowed_tools_args(None).is_empty());
    assert!(role_allowed_tools_args(Some("")).is_empty());
    assert!(role_allowed_tools_args(Some("some-other-role")).is_empty());
}

const A_MAIN: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
const A_MINTED_TOKEN: &str = "minted-for-the-agent";

/// A main's launch on `config` / `data_dir`, written into a fresh session dir.
struct AMainLaunch {
    session_dir: tempfile::TempDir,
    launch: RoleLaunch,
}

impl AMainLaunch {
    fn on(config: &DaemonConfig, data_dir: &Path) -> Self {
        let session_dir = tempfile::tempdir().expect("a temporary session dir");
        let launch = project_home_launch(ProjectHomeLaunch {
            config,
            tddy_data_dir: data_dir,
            session_dir: session_dir.path(),
            session_id: A_MAIN,
            agent_session_token: A_MINTED_TOKEN,
            tddy_tools_path: Path::new("/opt/tddy/bin/tddy-tools"),
        })
        .expect("a main's launch is built");
        Self {
            session_dir,
            launch,
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
    let main = AMainLaunch::on(&host.config, host.data_dir());

    // Then its tools are pointed at that socket, which `tddy-tools` prefers over any URL
    assert_eq!(
        main.env("TDDY_REMOTE_DAEMON_SOCKET"),
        Some(host.agent_tool_socket().as_str())
    );
}

#[test]
fn on_a_regular_host_a_main_reaches_the_daemon_over_its_http_url_and_no_socket() {
    // Given a binary daemon, which serves HTTP and binds no agent-tool socket
    let host = ARegularHost::without_an_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, host.data_dir());

    // Then its tools reach this daemon's local URL, and name no socket nobody serves
    assert_eq!(
        main.env("TDDY_REMOTE_DAEMON_URL"),
        Some(daemon_hook_urls::local_daemon_hook_url(&host.config).as_str())
    );
    assert_eq!(main.env("TDDY_REMOTE_DAEMON_SOCKET"), None);
}

#[test]
fn a_mains_tools_carry_its_own_session_id_and_the_minted_token() {
    // Given a binary daemon
    let host = ARegularHost::without_an_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, host.data_dir());

    // Then every tool call names the main itself, under the credential minted for it
    assert_eq!(main.env("TDDY_REMOTE_SESSION_ID"), Some(A_MAIN));
    assert_eq!(main.env("TDDY_REMOTE_SESSION_TOKEN"), Some(A_MINTED_TOKEN));
}

#[test]
fn a_main_loads_only_the_tddy_tools_mcp_server_from_its_session_dir() {
    // Given a binary daemon
    let host = ARegularHost::without_an_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, host.data_dir());

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
    let host = ARegularHost::without_an_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, host.data_dir());

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
    let host = ARegularHost::without_an_agent_tool_socket();

    // When a main's launch is built there
    let main = AMainLaunch::on(&host.config, host.data_dir());

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
    record_role_before_spawn(
        dir.path(),
        "home-1",
        "p-1",
        "project-home",
        Path::new("/work/home"),
        true,
    )
    .expect("recorded");

    // Then the record names the role, the project and the worktree, with no pid yet
    let record = tddy_core::read_session_metadata(dir.path()).expect("the record is readable");
    assert_eq!(record.session_role.as_deref(), Some("project-home"));
    assert_eq!(record.project_id, "p-1");
    assert_eq!(record.repo_path.as_deref(), Some("/work/home"));
    assert_eq!(record.pid, None);
}

#[test]
fn an_ordinary_session_writes_nothing_before_the_spawn() {
    // Given a session directory a start has just created
    let dir = tempfile::tempdir().expect("a temporary session dir");

    // When a session with no role has its role recorded ahead of the spawn
    record_role_before_spawn(dir.path(), "s-1", "p-1", "", Path::new("/work"), false)
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

/// The notify environment a session on a host that serves HTTP RPC at `daemon_url` is given.
fn the_environment_reaching_the_url(daemon_url: &str) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        (notify_env::SESSION_ID, A_NOTIFYING_SESSION.to_string()),
        (notify_env::OS_USER, ITS_OS_USER.to_string()),
        (notify_env::HOOK_TOKEN, ITS_HOOK_TOKEN.to_string()),
        (notify_env::DAEMON_URL, daemon_url.to_string()),
    ])
}

fn as_borrowed<'a>(expected: &'a BTreeMap<&'static str, String>) -> BTreeMap<&'a str, &'a str> {
    expected.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// A plain claude-cli start's notify wiring on `config` / `data_dir`, with no managed recipe.
fn a_started_sessions_notify_wiring(
    config: &DaemonConfig,
    data_dir: &Path,
    session_dir: &Path,
) -> TheNotifyWiring {
    TheNotifyWiring(
        agent_notify_launch(AgentNotifyLaunch {
            config,
            tddy_data_dir: data_dir,
            os_user: ITS_OS_USER,
            session_id: A_NOTIFYING_SESSION,
            hook_token: ITS_HOOK_TOKEN,
            session_dir,
            managed_prompt_file: None,
        })
        .expect("a start's notify wiring is built"),
    )
}

#[test]
fn a_claude_cli_session_is_launched_with_the_notify_environment_and_the_guidance_fragment() {
    // Given a binary daemon, which serves HTTP RPC
    let host = ARegularHost::without_an_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");

    // When a claude-cli session is started there
    let wiring =
        a_started_sessions_notify_wiring(&host.config, host.data_dir(), session_dir.path());

    // Then its agent's environment names its session, its owner, its hook token and the daemon's
    // URL — exactly what `tddy-tools notify` reads
    let expected =
        the_environment_reaching_the_url(&daemon_hook_urls::claude_hook_daemon_url(&host.config));
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
    let host = ARegularHost::without_an_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");
    let recorded = tddy_core::SessionMetadata {
        hook_token: Some(ITS_HOOK_TOKEN.to_string()),
        ..service_util::starting_session_metadata(A_NOTIFYING_SESSION, "p-1", "claude-cli")
    };

    // When it is resumed
    let wiring = TheNotifyWiring(
        resumed_agent_notify_launch(
            &host.config,
            host.data_dir(),
            ITS_OS_USER,
            session_dir.path(),
            &recorded,
            None,
        )
        .expect("a resume's notify wiring is built"),
    );

    // Then its new process is given the same notify environment its first one was
    let expected =
        the_environment_reaching_the_url(&daemon_hook_urls::claude_hook_daemon_url(&host.config));
    assert_eq!(wiring.environment(), as_borrowed(&expected));
}

#[test]
fn an_embedded_host_exports_the_daemon_socket_not_the_url() {
    // Given a desktop daemon, which serves no HTTP RPC and has its agent-tool socket bound
    let host = AnEmbeddedHost::with_its_agent_tool_socket_bound();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");

    // When a claude-cli session is started there
    let wiring =
        a_started_sessions_notify_wiring(&host.config, host.data_dir(), session_dir.path());

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
    let host = ARegularHost::without_an_agent_tool_socket();
    let session_dir = tempfile::tempdir().expect("a temporary session dir");
    let recipe_prompt = session_dir.path().join("recipe-prompt.md");
    std::fs::write(&recipe_prompt, "You are driving the tdd recipe.").expect("the recipe prompt");

    // When the session is started — `claude` takes a single `--append-system-prompt-file`
    let wiring = TheNotifyWiring(
        agent_notify_launch(AgentNotifyLaunch {
            config: &host.config,
            tddy_data_dir: host.data_dir(),
            os_user: ITS_OS_USER,
            session_id: A_NOTIFYING_SESSION,
            hook_token: ITS_HOOK_TOKEN,
            session_dir: session_dir.path(),
            managed_prompt_file: Some(&recipe_prompt),
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
