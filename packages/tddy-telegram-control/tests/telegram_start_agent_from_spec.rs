//! Integration: a Telegram start of a CLI agent goes through the daemon's one generic start — the
//! spec launch — so it is indistinguishable from a web start: the argv the agent's spec renders,
//! its hooks file and hook token, the start's own branch-conflict guard, and the agents and models
//! the host's coding-agent registry offers (`#agent-tools 18/19`).
//!
//! Feature doc: `docs/ft/daemon/telegram-session-control.md`.
//!
//! The world is the daemon in miniature: a real `DaemonSessionHost` given its coding agents as the
//! runtime gives them — the registry over a `<tddyhome>/tools` directory, resolved against a fixture
//! host whose environment names each agent's stub through the spec's own `binary.env` variable — and
//! a Telegram harness holding **the same** registry and the host's session starter. A start is driven
//! through the harness's handlers exactly as the bot dispatches the operator's taps. Every stub
//! records its argv and the names of its `TDDY_*` variables into a directory of its own, then waits
//! on stdin so the session stays up.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::load_agent_registry;
use tddy_core::changeset::Changeset;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::operator_session_starter::HostSessionStarter;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_telegram_control::telegram_notifier::{session_telegram_label, InMemoryTelegramSender};
use tddy_telegram_control::telegram_session_control::{
    collect_outbound_messages, CapturedTelegramMessage, StartAgentCommand, StartClaudeCommand,
    StartCursorCommand, TelegramSessionControlHarness, TelegramWorkflowSpawn,
};
use tddy_testing_commons::wait::eventually;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const AUTHORIZED_CHAT: i64 = 777_018;
const TELEGRAM_USER: u64 = 42;
const TEST_PROJECT_ID: &str = "tg-spec-launch-project";
const PROMPT: &str = "Auth Feature";
/// `feature/<slug(changeset name)>` for [`PROMPT`] — what a `branch_name_required` agent's start
/// names.
const DERIVED_BRANCH: &str = "feature/auth-feature";
/// The chat the cursor stub's `create-chat` mints.
const MINTED_CHAT_ID: &str = "tg-cursor-chat-0001";
const OWNER_SESSION: &str = "019d6392-3cff-0018-aaaa-000000000001";
/// How long a stub has to record its launch: the start sets up a git worktree, renders the spec's
/// slots and spawns the PTY before the stub runs — over the 1s integration budget, well under 10s.
const LAUNCH_RECORDED: Duration = Duration::from_secs(10);

/// `acme`: an agent no Rust names, declared in this one file dropped into `<tddyhome>/tools/`.
const ACME_YAML: &str = r#"schema_version: 1
id: acme
summary: Acme Code — an agent added as a tool file alone
binary:
  command: acme
  env: TDDY_ACME_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
    - { if: prompt, then: ["{prompt}"] }
  env: {}
extensions:
  coding_agent:
    mcp: { kind: unsupported }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: unsupported }
    capabilities: [requires_model]
    models:
      default: acme-1
      curated: [acme-1]
    placements: [host]
"#;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The OS user the test process runs as, so a host spawn needs no privilege drop.
fn current_os_user() -> String {
    let out = std::process::Command::new("id")
        .arg("-un")
        .output()
        .expect("`id -un` runs");
    String::from_utf8(out.stdout)
        .expect("a user name is UTF-8")
        .trim()
        .to_string()
}

fn make_executable(path: &Path) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The real `tddy-tools`, built beside this test binary (`./test` builds it first).
fn tddy_tools_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
}

/// A directory of stub agents — with the real `tddy-tools` linked in, it is the fixture host's
/// whole `PATH` — and the launches they record.
struct AStubBin {
    dir: tempfile::TempDir,
}

fn a_stub_bin() -> AStubBin {
    let stubs = AStubBin {
        dir: tempfile::tempdir().unwrap(),
    };
    std::os::unix::fs::symlink(tddy_tools_binary(), stubs.binary("tddy-tools")).unwrap();
    stubs
        .with_recording_agent("claude")
        .with_recording_agent("agent")
        .with_recording_agent("acme")
}

impl AStubBin {
    fn path(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).unwrap()
    }

    /// The stub's canonical path — the one the launch renders into the argv (a macOS tempdir is
    /// reached through the `/private` symlink).
    fn binary(&self, name: &str) -> PathBuf {
        self.path().join(name)
    }

    fn records_of(&self, name: &str) -> PathBuf {
        self.dir.path().join(format!("{name}.records"))
    }

    /// A stub agent `name`: run as `<name> create-chat` it prints [`MINTED_CHAT_ID`] and exits (the
    /// cursor spec's minted conversation); otherwise it records its argv (one entry per line, `$0`
    /// first) and the names of its `TDDY_*` variables, then waits on stdin.
    fn with_recording_agent(self, name: &str) -> Self {
        let records = self.records_of(name);
        std::fs::create_dir(&records).unwrap();
        let script = self.binary(name);
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"create-chat\" ]; then printf '%s\\n' '{MINTED_CHAT_ID}'; exit 0; fi\n\
                 /usr/bin/env | /usr/bin/sed -n 's/^\\(TDDY_[A-Za-z0-9_]*\\)=.*/\\1/p' > '{records}/tddy-env'\n\
                 printf '%s\\n' \"$0\" \"$@\" > '{records}/argv.partial'\n\
                 /bin/mv '{records}/argv.partial' '{records}/argv'\n\
                 exec /bin/cat\n",
                records = records.display(),
            ),
        )
        .unwrap();
        make_executable(&script);
        self
    }

    /// Whether stub `name` was launched as an agent (a `create-chat` mint is not a launch).
    fn was_launched(&self, name: &str) -> bool {
        self.records_of(name).join("argv").exists()
    }

    /// What stub `name` recorded, once it has.
    async fn launch_of(&self, name: &str) -> RecordedLaunch {
        let records = self.records_of(name);
        eventually(
            &format!("stub {name} records its launch"),
            LAUNCH_RECORDED,
            || {
                let argv = std::fs::read_to_string(records.join("argv"))
                    .map_err(|e| format!("no argv yet: {e}"))?;
                let tddy_env = std::fs::read_to_string(records.join("tddy-env"))
                    .map_err(|e| format!("no environment yet: {e}"))?;
                Ok(RecordedLaunch {
                    argv: argv.lines().map(str::to_string).collect(),
                    tddy_variables: tddy_env.lines().map(str::to_string).collect(),
                })
            },
        )
        .await
    }
}

/// What a stub agent was launched with.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedLaunch {
    argv: Vec<String>,
    tddy_variables: Vec<String>,
}

impl RecordedLaunch {
    /// The first `n` argv entries — what the spec renders before its slot flags.
    fn argv_head(&self, n: usize) -> Vec<String> {
        self.argv.iter().take(n).cloned().collect()
    }

    /// The last argv entry — the spec's `{prompt}`.
    fn argv_tail(&self) -> String {
        self.argv.last().cloned().unwrap_or_default()
    }
}

/// A host whose environment and `PATH` are declared — nothing of the real machine's `PATH` or
/// `TDDY_*_BINARY` variables reaches resolution. Each agent's stub is named through the variable
/// its spec's `binary.env` declares.
struct AFixtureHost {
    path: Vec<PathBuf>,
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// The coding agents a daemon would install from `tools` (`<tddyhome>/tools`), each resolving to
/// its stub in `stubs`.
fn the_coding_agents(tools: &Path, stubs: &AStubBin) -> Arc<HostCodingAgents> {
    let registry = load_agent_registry(tools).expect("the agent registry loads");
    let env = [
        ("TDDY_CLAUDE_BINARY", "claude"),
        ("TDDY_CURSOR_AGENT", "agent"),
        ("TDDY_ACME_CLI", "acme"),
    ]
    .into_iter()
    .map(|(var, stub)| (var.to_string(), stubs.binary(stub).display().to_string()))
    .collect();
    Arc::new(HostCodingAgents::new(
        registry,
        Arc::new(AFixtureHost {
            path: vec![stubs.path()],
            env,
        }),
    ))
}

/// A git repo with `origin/main`, registered as [`TEST_PROJECT_ID`] in `projects_dir`.
fn a_registered_project(projects_dir: &Path) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    };
    git(&["init", "-b", "main"]);
    git(&["commit", "--allow-empty", "-m", "init"]);
    git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    git(&["push", "-u", "origin", "main"]);
    std::fs::create_dir_all(projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: {TEST_PROJECT_ID}\n    git_url: \"\"\n    main_repo_path: {}\n    main_branch_ref: origin/main\n",
            repo.path().display()
        ),
    )
    .unwrap();
    repo
}

/// A daemon config that maps this process's user to itself and names no agent section.
fn a_daemon_config() -> DaemonConfig {
    let user = current_os_user();
    serde_yaml::from_str(&format!(
        "users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\n"
    ))
    .expect("the fixture daemon config parses")
}

/// How a [`World`] is built: which `<tddyhome>/tools` files it holds and whether its host has
/// coding agents at all.
struct AWorldBuilder {
    tools: Vec<(&'static str, &'static str)>,
    with_coding_agents: bool,
}

fn a_world() -> AWorldBuilder {
    AWorldBuilder {
        tools: Vec::new(),
        with_coding_agents: true,
    }
}

impl AWorldBuilder {
    /// `<tddyhome>/tools/<file>` holds `yaml`.
    fn with_tool_file(mut self, file: &'static str, yaml: &'static str) -> Self {
        self.tools.push((file, yaml));
        self
    }

    /// A host whose coding agents did not load — and so a Telegram harness with none to offer.
    fn without_coding_agents(mut self) -> Self {
        self.with_coding_agents = false;
        self
    }

    fn build(self) -> World {
        let tddy_home = tempfile::tempdir().unwrap();
        let tools = tddy_home.path().join("tools");
        std::fs::create_dir(&tools).unwrap();
        for (file, yaml) in &self.tools {
            std::fs::write(tools.join(file), yaml).unwrap();
        }
        let stubs = a_stub_bin();
        let sessions_base = tddy_home.path().join("data");
        std::fs::create_dir_all(sessions_base.join(SESSIONS_SUBDIR)).unwrap();
        let projects_dir = sessions_base.join("projects");
        let repo = a_registered_project(&projects_dir);
        let coding_agents = self
            .with_coding_agents
            .then(|| the_coding_agents(&tools, &stubs));

        let config = a_daemon_config();
        let manager = Arc::new(ClaudeCliSessionManager::new());
        let host_base = sessions_base.clone();
        let host = DaemonSessionHost::new(
            config.clone(),
            Arc::new(move |_| Some(host_base.clone())),
            sessions_base.clone(),
            // Nobody signs in over a token here: the Telegram start is keyed by its OS user.
            Arc::new(|_| None),
            None,
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
            Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
            Arc::clone(&manager),
        );
        let host = Arc::new(match &coding_agents {
            Some(agents) => host.with_coding_agents(Arc::clone(agents)),
            None => host,
        });
        host.install_sandbox_rpc_bridge();

        let sender = Arc::new(InMemoryTelegramSender::new());
        let workflow_spawn = Arc::new(TelegramWorkflowSpawn {
            config: Arc::new(config),
            spawn_client: None,
            spawn_backend: tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
            os_user: current_os_user(),
            tddy_data_dir: sessions_base.clone(),
            projects_dir_override: Some(projects_dir),
            telegram_hooks: None,
            child_grpc_by_session: Default::default(),
            elicitation_select_options: Default::default(),
            elicitation_multi_select_meta: Default::default(),
            pending_elicitation_other: Default::default(),
            claude_cli_manager: manager,
        });
        let registry = coding_agents
            .as_ref()
            .map(|agents| agents.agents_shared())
            .unwrap_or_else(|| Arc::new(tddy_coding_agent_specs::CodingAgentRegistry::empty()));
        let harness = TelegramSessionControlHarness::with_workflow_spawn(
            vec![AUTHORIZED_CHAT],
            sessions_base.clone(),
            Arc::clone(&sender),
            Some(workflow_spawn),
            None,
        )
        .with_coding_agents(registry)
        .with_session_starter(Arc::new(HostSessionStarter::new(Arc::clone(&host))));

        World {
            _tddy_home: tddy_home,
            _repo: repo,
            stubs,
            sessions_base,
            harness,
            sender,
            _host: host,
        }
    }
}

/// The daemon in miniature, and the chat driving it.
struct World {
    _tddy_home: tempfile::TempDir,
    _repo: tempfile::TempDir,
    stubs: AStubBin,
    sessions_base: PathBuf,
    harness: TelegramSessionControlHarness<InMemoryTelegramSender>,
    sender: Arc<InMemoryTelegramSender>,
    _host: Arc<DaemonSessionHost>,
}

impl World {
    /// `/start-claude <prompt>`; the pending session's id.
    async fn start_claude(&mut self, prompt: &str) -> anyhow::Result<String> {
        self.harness
            .handle_start_claude(StartClaudeCommand {
                chat_id: AUTHORIZED_CHAT,
                user_id: TELEGRAM_USER,
                prompt: prompt.to_string(),
            })
            .await
            .map(|outcome| outcome.session_id)
    }

    /// `/start-cursor <prompt>`; the pending session's id.
    async fn start_cursor(&mut self, prompt: &str) -> String {
        self.harness
            .handle_start_cursor(StartCursorCommand {
                chat_id: AUTHORIZED_CHAT,
                user_id: TELEGRAM_USER,
                prompt: prompt.to_string(),
            })
            .await
            .expect("/start-cursor is answered")
            .session_id
    }

    /// `/start-agent <prompt>`; the pending session's id.
    async fn start_agent(&mut self, prompt: &str) -> String {
        self.harness
            .handle_start_agent(StartAgentCommand {
                chat_id: AUTHORIZED_CHAT,
                user_id: TELEGRAM_USER,
                prompt: prompt.to_string(),
            })
            .await
            .expect("/start-agent is answered")
            .session_id
    }

    /// Tap the agent button labelled `agent_id` of the last agent keyboard.
    async fn pick_agent(&self, pending: &str, agent_id: &str) {
        let agent_idx = self
            .last_keyboard_labels()
            .iter()
            .position(|label| label == agent_id)
            .unwrap_or_else(|| {
                panic!(
                    "no {agent_id} button on the agent keyboard {:?}",
                    self.last_keyboard_labels()
                )
            });
        self.harness
            .handle_telegram_agent_pick_callback(AUTHORIZED_CHAT, agent_idx, pending)
            .await
            .expect("the agent pick is answered");
    }

    /// Tap the project, then the project's default base branch.
    async fn pick_project_and_default_branch(&self, pending: &str) {
        self.harness
            .handle_telegram_project_callback(AUTHORIZED_CHAT, 0, pending)
            .await
            .expect("the project pick is answered");
        self.harness
            .handle_telegram_branch_callback(AUTHORIZED_CHAT, 0, 0, 0, pending)
            .await
            .expect("the branch pick is answered");
    }

    /// Tap the model button at `model_idx`.
    async fn pick_model(&self, pending: &str, model_idx: usize) -> anyhow::Result<()> {
        self.harness
            .handle_telegram_model_pick_callback(AUTHORIZED_CHAT, model_idx, 0, pending)
            .await
    }

    fn messages(&self) -> Vec<CapturedTelegramMessage> {
        collect_outbound_messages(&self.sender, AUTHORIZED_CHAT)
    }

    fn last_keyboard_labels(&self) -> Vec<String> {
        self.messages()
            .iter()
            .rev()
            .find(|message| !message.inline_keyboard.is_empty())
            .map(|message| {
                message
                    .inline_keyboard
                    .iter()
                    .flatten()
                    .map(|(label, _)| label.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn last_message(&self) -> CapturedTelegramMessage {
        self.messages()
            .pop()
            .expect("the chat was sent at least one message")
    }

    /// The sessions under the sessions base that were started — those with a `.session.yaml`.
    fn started_sessions(&self) -> Vec<SessionMetadata> {
        let mut started: Vec<SessionMetadata> =
            std::fs::read_dir(self.sessions_base.join(SESSIONS_SUBDIR))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter_map(|dir| read_session_metadata(&dir).ok())
                .collect();
        started.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        started
    }

    /// The one session the start started.
    fn the_started_session(&self) -> SessionMetadata {
        let started = self.started_sessions();
        assert_eq!(
            started.len(),
            1,
            "exactly one session was started; found {started:?}"
        );
        started.into_iter().next().unwrap()
    }

    /// The sessions directories under the sessions base, by id.
    fn session_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<String> = std::fs::read_dir(self.sessions_base.join(SESSIONS_SUBDIR))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        dirs.sort();
        dirs
    }

    /// A live session owning `branch`, as another start would have left it.
    fn a_session_owning(&self, session_id: &str, branch: &str) {
        let dir = self.sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        tddy_core::write_changeset(
            &dir,
            &Changeset {
                branch: Some(branch.to_string()),
                ..Changeset::default()
            },
        )
        .unwrap();
        let meta = a_session_metadata()
            .with_session_id(session_id)
            .with_status("active")
            // The test process is the only pid guaranteed alive, so it is what makes it active.
            .with_pid(std::process::id())
            .build();
        write_session_yaml(&dir, &meta);
    }
}

/// The file `relative` in the started session's worktree.
fn worktree_file(session: &SessionMetadata, relative: &str) -> String {
    let worktree = session
        .repo_path
        .as_deref()
        .expect("a started session records its worktree");
    std::fs::read_to_string(Path::new(worktree).join(relative))
        .unwrap_or_else(|e| panic!("{relative} in the worktree {worktree}: {e}"))
}

/// Whether the hooks file `relative` of `session`'s worktree carries the session's hook token.
fn hooks_file_carries_the_hook_token(session: &SessionMetadata, relative: &str) -> bool {
    let token = session
        .hook_token
        .as_deref()
        .expect("a hooked session records its hook token");
    worktree_file(session, relative).contains(token)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_claude_start_launches_the_argv_the_claude_spec_renders_with_its_hooks_file_and_hook_token(
) {
    // Given
    let mut world = a_world().build();
    let pending = world
        .start_claude(PROMPT)
        .await
        .expect("/start-claude is answered");
    world.pick_project_and_default_branch(&pending).await;

    // When — the first model: the spec's `models.default`
    world
        .pick_model(&pending, 0)
        .await
        .expect("the start succeeds");

    // Then — the claude spec's argv, its hooks file holding the session's hook token, and no
    // TDDY_* variable in the agent's environment
    let session = world.the_started_session();
    let launch = world.stubs.launch_of("claude").await;
    assert_eq!(
        (
            launch.argv_head(5),
            launch.argv_tail(),
            launch.tddy_variables.clone(),
            session.session_type.clone(),
            session.model.clone(),
            hooks_file_carries_the_hook_token(&session, ".claude/settings.local.json"),
        ),
        (
            vec![
                world.stubs.binary("claude").display().to_string(),
                "--model".to_string(),
                "opus".to_string(),
                "--session-id".to_string(),
                session.session_id.clone(),
            ],
            PROMPT.to_string(),
            Vec::<String>::new(),
            Some("claude-cli".to_string()),
            Some("opus".to_string()),
            true,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_cursor_start_launches_from_the_cursor_spec_with_a_minted_chat_id() {
    // Given
    let mut world = a_world().build();
    let pending = world.start_cursor(PROMPT).await;
    world.pick_project_and_default_branch(&pending).await;

    // When — the first model: cursor.yaml's `models.default`
    world
        .pick_model(&pending, 0)
        .await
        .expect("the start succeeds");

    // Then — the cursor spec's argv resuming the chat the start minted, which the session records,
    // and its hooks file holding the hook token
    let session = world.the_started_session();
    let launch = world.stubs.launch_of("agent").await;
    assert_eq!(
        (
            launch.argv_head(5),
            launch.argv_tail(),
            launch.tddy_variables.clone(),
            session.session_type.clone(),
            session.cursor_chat_id.clone(),
            hooks_file_carries_the_hook_token(&session, ".cursor/hooks.json"),
        ),
        (
            vec![
                world.stubs.binary("agent").display().to_string(),
                "--resume".to_string(),
                MINTED_CHAT_ID.to_string(),
                "--model".to_string(),
                "composer-2.5".to_string(),
            ],
            PROMPT.to_string(),
            Vec::<String>::new(),
            Some("cursor-cli".to_string()),
            Some(MINTED_CHAT_ID.to_string()),
            true,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_start_of_an_agent_dropped_into_tddyhome_tools_launches_it() {
    // Given — acme.yaml in `<tddyhome>/tools`, and nothing else about acme anywhere
    let mut world = a_world().with_tool_file("acme.yaml", ACME_YAML).build();
    let pending = world.start_agent(PROMPT).await;
    world.pick_agent(&pending, "acme").await;
    world.pick_project_and_default_branch(&pending).await;

    // When
    world
        .pick_model(&pending, 0)
        .await
        .expect("the start succeeds");

    // Then — started as acme-cli with acme.yaml's argv
    let session = world.the_started_session();
    let launch = world.stubs.launch_of("acme").await;
    assert_eq!(
        (launch.argv.clone(), session.session_type.clone()),
        (
            vec![
                world.stubs.binary("acme").display().to_string(),
                "--model".to_string(),
                "acme-1".to_string(),
                PROMPT.to_string(),
            ],
            Some("acme-cli".to_string()),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_telegram_agent_picker_lists_the_registrys_cli_agents_and_the_model_picker_lists_the_chosen_specs_models(
) {
    // Given
    let mut world = a_world().build();

    // When — `/start-agent`, then cursor, the project and its default base
    let pending = world.start_agent(PROMPT).await;
    let agent_keyboard = world.last_keyboard_labels();
    world.pick_agent(&pending, "cursor").await;
    world.pick_project_and_default_branch(&pending).await;
    let model_keyboard = world.last_keyboard_labels();

    // Then — every built-in agent with a PTY run, by id (the ACP-only agents are not CLI agents),
    // then cursor.yaml's default and curated models, by id
    assert_eq!(
        (agent_keyboard, model_keyboard),
        (
            ["claude", "codex", "cursor", "muse", "opencode", "oz", "warp"]
                .map(str::to_string)
                .to_vec(),
            [
                "composer-2.5",
                "gpt-5.3-codex",
                "claude-4.6-sonnet-medium-thinking"
            ]
            .map(str::to_string)
            .to_vec(),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_start_on_a_host_without_coding_agents_is_refused_in_the_chat_spawning_nothing()
{
    // Given — a host whose coding agents did not load
    let mut world = a_world().without_coding_agents().build();

    // When
    let refused = world
        .start_claude(PROMPT)
        .await
        .map_err(|refusal| refusal.to_string());

    // Then — the refusal is the chat's reply (the bot sends a handler's error to the chat), and
    // nothing was created or launched
    assert_eq!(
        (
            refused,
            world.session_dirs(),
            world.stubs.was_launched("claude")
        ),
        (
            Err(
                "this host has no coding agents installed: Telegram cannot start a claude-cli \
                 session"
                    .to_string()
            ),
            Vec::<String>::new(),
            false,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_telegram_start_onto_a_branch_another_session_holds_reports_the_start_guards_conflict() {
    // Given — another session holds the branch the claude start will name
    let mut world = a_world().build();
    world.a_session_owning(OWNER_SESSION, DERIVED_BRANCH);
    let pending = world
        .start_claude(PROMPT)
        .await
        .expect("/start-claude is answered");
    world.pick_project_and_default_branch(&pending).await;

    // When — the model pick runs the start, whose guard finds the owner
    world
        .pick_model(&pending, 0)
        .await
        .expect("the conflict is answered");

    // Then — the three conflict choices name the owner, and nothing new was started or launched
    let prompt = world.last_message();
    let owner = session_telegram_label(OWNER_SESSION).expect("a uuid has a Telegram label");
    let started: Vec<String> = world
        .started_sessions()
        .into_iter()
        .map(|session| session.session_id)
        .collect();
    assert_eq!(
        (
            prompt.text,
            prompt
                .inline_keyboard
                .iter()
                .flatten()
                .map(|(label, _)| label.clone())
                .collect::<Vec<_>>(),
            started,
            world.stubs.was_launched("claude"),
        ),
        (
            format!(
                "Branch `{DERIVED_BRANCH}` is already used by session {owner}. Choose how to continue:"
            ),
            vec![
                format!("Switch to {owner}"),
                format!("New agent on {DERIVED_BRANCH}"),
                format!("Use {DERIVED_BRANCH}-1"),
            ],
            vec![OWNER_SESSION.to_string()],
            false,
        )
    );
}
