//! A conversation or pr-stack child runs its parent session's agent (`#agent-tools 17/19`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`. The `pr_spawn_child` and
//! `spawn_conversation` tools name no agent, so the child takes the one its orchestrator's
//! `.session.yaml` records — resolved through the host's installed coding agents, never a claude
//! default — and a host with no coding agents refuses the child `FAILED_PRECONDITION`.
//!
//! The handlers are the real ones, wired as a pr-stack or grill-me start wires them, over the real
//! `DaemonSessionHost`. Its coding agents are the built-in specs on a fixture host whose `PATH`
//! holds a stub for each built-in binary (`claude`, `agent`) and `tddy-tools`; each stub appends
//! its own name to one log when it is launched, so a test reads which agent the child ran.
//! In-crate rather than under `tests/`: the handlers are built from
//! `DaemonSessionHost::launch_sessions`, which is `pub(crate)`.

use std::os::unix::fs::PermissionsExt as _;

use super::session_acting_identity::SessionAccountAccess;
use super::*;
use tddy_actions::tool::ToolHost;
use tddy_core::toolcall::{ChildSpawnHandler, ConversationSpawnHandler};
use tddy_core::{Stack, StackNode};
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_testing_commons::wait::eventually;

const VALID_TOKEN: &str = "child-agent-token";
const TEST_PROJECT_ID: &str = "child-agent-project";
const ORCHESTRATOR_SESSION_ID: &str = "018f7777-cccc-7000-3333-000000000017";
const NODE_ID: &str = "n1";
/// A model the cursor spec curates: the child inherits its orchestrator's.
const CURSOR_MODEL: &str = "composer-2.5";
/// The chat the cursor stub's `create-chat` mints.
const MINTED_CHAT: &str = "child-chat-0001";

/// How long a stub has to record its launch. A safety net, not a prediction: it covers fork/exec
/// and the worktree cut under a loaded suite.
const AGENT_SPAWN: Duration = Duration::from_secs(10);

// ── The orchestrator ─────────────────────────────────────────────────────────────────────

/// A host with a declared `PATH` over the real filesystem: the stubs, and nothing of this machine's.
struct AStubPathHost {
    dir: PathBuf,
}

impl ToolHost for AStubPathHost {
    fn env_var(&self, name: &str) -> Option<String> {
        (name == "HOME").then(|| self.dir.to_string_lossy().into_owned())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![self.dir.clone()]
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// An orchestrator session of `session_type` on a daemon that can really start its child.
struct AnOrchestrator {
    service: DaemonSessionHost,
    config: DaemonConfig,
    os_user: String,
    sessions: tempfile::TempDir,
    /// One line per launched agent: the stub's name (`claude`, `agent`).
    launched: PathBuf,
    _repo: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

/// Which coding agents the orchestrator's host was given.
enum HostAgents {
    /// The built-in specs, their binaries the stubs.
    TheBuiltins,
    /// None at all.
    None,
}

/// An executable `name` in `dir` that appends `name` to `launched` and waits on stdin; run as
/// `<name> create-chat` it prints [`MINTED_CHAT`] instead (cursor's mint).
fn a_stub_agent(dir: &Path, name: &str, launched: &Path) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then printf '%s\\n' '{MINTED_CHAT}'; exit 0; fi\nprintf '%s\\n' '{name}' >> '{}'\nexec /bin/cat\n",
            launched.display()
        ),
    )
    .expect("write the stub");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("the stub is executable");
    path
}

fn an_orchestrator_of(session_type: &str, recipe: &str, agents: HostAgents) -> AnOrchestrator {
    let os_user = crate::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
        .expect("the test process's uid resolves to a passwd entry");

    let repo = tempfile::tempdir().expect("repo dir");
    create_repo_with_origin(repo.path());
    let sessions = tempfile::tempdir().expect("sessions dir");
    register_project(&sessions.path().join("projects"), repo.path());

    let stub_dir = tempfile::tempdir().expect("stub dir");
    let stubs = std::fs::canonicalize(stub_dir.path()).expect("stub dir is canonical");
    let launched = stubs.join("launched.log");
    let claude = a_stub_agent(&stubs, "claude", &launched);
    a_stub_agent(&stubs, "agent", &launched);
    std::os::unix::fs::symlink(the_tddy_tools_binary(), stubs.join("tddy-tools"))
        .expect("tddy-tools beside the stubs");

    // The daemon config still names a claude binary — the hand-built launch's — so a child that
    // fell back to it would run the claude stub and be seen.
    let config_dir = tempfile::tempdir().expect("config dir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!(
            "users:\n  - github_user: \"{os_user}\"\n    os_user: \"{os_user}\"\nclaude_cli:\n  binary_path: {}\n",
            claude.display()
        ),
    )
    .expect("write daemon config");
    let config = crate::test_util::daemon_config_from_fixture_file(&config_path)
        .expect("daemon config parses");

    let sessions_base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = os_user.clone();
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
    let host = DaemonSessionHost::new(
        config.clone(),
        resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        crate::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
    );
    let service = match agents {
        HostAgents::TheBuiltins => {
            let no_overrides = tempfile::tempdir().expect("an empty tools dir");
            host.with_coding_agents(Arc::new(
                tddy_agent_launch::spec_launch::HostCodingAgents::new(
                    tddy_coding_agent_specs::load_agent_registry(no_overrides.path())
                        .expect("the built-in coding agents load"),
                    Arc::new(AStubPathHost { dir: stubs.clone() }),
                ),
            ))
        }
        HostAgents::None => host,
    };

    let orchestrator = AnOrchestrator {
        service,
        config,
        os_user,
        sessions,
        launched,
        _repo: repo,
        _config_dir: config_dir,
        _stub_dir: stub_dir,
    };
    orchestrator.write_session(session_type, recipe);
    orchestrator
}

impl AnOrchestrator {
    fn sessions_base(&self) -> &Path {
        self.sessions.path()
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions_base(), session_id)
    }

    /// The orchestrator's `.session.yaml` — its session type, recipe and model — and a planned
    /// stack of one undeveloped node.
    fn write_session(&self, session_type: &str, recipe: &str) {
        let dir = self.session_dir(ORCHESTRATOR_SESSION_ID);
        std::fs::create_dir_all(dir.join("artifacts")).expect("orchestrator artifacts");
        std::fs::write(
            dir.join(".session.yaml"),
            format!(
                "session_id: {ORCHESTRATOR_SESSION_ID}\nproject_id: {TEST_PROJECT_ID}\n\
                 created_at: 2026-11-05T10:00:00Z\nupdated_at: 2026-11-05T10:00:00Z\n\
                 status: active\nsession_type: {session_type}\nrecipe: {recipe}\nmodel: {CURSOR_MODEL}\n"
            ),
        )
        .expect("write orchestrator metadata");
        tddy_core::write_changeset(
            &dir,
            &Changeset {
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![StackNode {
                        node_id: NODE_ID.to_string(),
                        title: "Token store".to_string(),
                        description: "Adds the token store.".to_string(),
                        branch_suggestion: Some("feature/token-store".to_string()),
                        ..Default::default()
                    }],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
    }

    /// The handler the `pr_spawn_child` tool reaches, wired as a pr-stack start wires it.
    fn child_spawn_handler(&self) -> StackChildSpawnHandler {
        StackChildSpawnHandler {
            stack_parent_host: Arc::new(self.service.launch_sessions()),
            service: self.service.launch_sessions(),
            config: self.config.clone(),
            tddy_data_dir: self.sessions_base().to_path_buf(),
            claude_cli_manager: Arc::clone(&self.service.claude_cli_manager),
            os_user: self.os_user.clone(),
            project_id: TEST_PROJECT_ID.to_string(),
            sessions_base: self.sessions_base().to_path_buf(),
            orchestrator_session_id: ORCHESTRATOR_SESSION_ID.to_string(),
            orchestrator_session_dir: self.session_dir(ORCHESTRATOR_SESSION_ID),
            account_access: SessionAccountAccess::new(None, Arc::new(|_| None), VALID_TOKEN),
        }
    }

    /// The handler the `spawn_conversation` tool reaches, wired as a grill-me start wires it.
    fn conversation_spawn_handler(&self) -> GrillMeConversationSpawnHandler {
        GrillMeConversationSpawnHandler {
            service: self.service.launch_sessions(),
            stack_parent_host: Arc::new(self.service.launch_sessions()),
            config: self.config.clone(),
            tddy_data_dir: self.sessions_base().to_path_buf(),
            claude_cli_manager: Arc::clone(&self.service.claude_cli_manager),
            os_user: self.os_user.clone(),
            project_id: TEST_PROJECT_ID.to_string(),
            sessions_base: self.sessions_base().to_path_buf(),
            orchestrator_session_id: ORCHESTRATOR_SESSION_ID.to_string(),
            orchestrator_session_dir: self.session_dir(ORCHESTRATOR_SESSION_ID),
            model_override: None,
            account_access: SessionAccountAccess::new(None, Arc::new(|_| None), VALID_TOKEN),
        }
    }

    /// The session type a spawned child's `.session.yaml` records.
    fn session_type_of(&self, session_id: &str) -> Option<String> {
        tddy_core::read_session_metadata(&self.session_dir(session_id))
            .expect("the child records its metadata")
            .session_type
    }

    /// The stub that ran as the child's agent, once one has recorded its launch.
    async fn the_agent_launched(&self) -> String {
        let launched = self.launched.clone();
        eventually(
            "the child's agent to record its launch",
            AGENT_SPAWN,
            || {
                std::fs::read_to_string(&launched)
                    .map(|log| log.trim().to_string())
                    .map_err(|e| format!("no agent has recorded a launch yet: {e}"))
                    // The stub creates its log before it writes its name to it.
                    .and_then(|log| {
                        (!log.is_empty())
                            .then_some(log)
                            .ok_or_else(|| "the launch log is still empty".to_string())
                    })
            },
        )
        .await
    }

    /// Every agent launched so far, read now.
    fn agents_launched_so_far(&self) -> String {
        std::fs::read_to_string(&self.launched).unwrap_or_default()
    }

    /// The sessions under the sessions base other than the orchestrator.
    fn other_sessions(&self) -> Vec<String> {
        std::fs::read_dir(self.sessions_base().join("sessions"))
            .expect("the sessions dir")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name != ORCHESTRATOR_SESSION_ID)
            .collect()
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────────────────────

/// The real `tddy-tools`, built beside the test binaries.
fn the_tddy_tools_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
}

fn create_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command must run");
    };
    run(&["init", "-b", "main"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().expect("repo path")]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo: &Path) {
    std::fs::create_dir_all(projects_dir).expect("projects dir");
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: child-agent\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo.display()
        ),
    )
    .expect("write projects.yaml");
}

// ── The child's agent is the parent's ────────────────────────────────────────────────────

#[tokio::test]
async fn a_pr_stack_child_of_a_cursor_cli_session_starts_a_cursor_cli_child() {
    // Given — a cursor-cli pr-stack orchestrator on a host with the built-in coding agents
    let orchestrator = an_orchestrator_of("cursor-cli", "pr-stack", HostAgents::TheBuiltins);

    // When — its agent spawns the planned node, naming no agent
    let child = orchestrator
        .child_spawn_handler()
        .spawn_child(NODE_ID)
        .await
        .expect("the planned node spawns a child");

    // Then — the child is a cursor-cli session, and Cursor's binary is what ran
    assert_eq!(
        (
            orchestrator.session_type_of(&child),
            orchestrator.the_agent_launched().await,
        ),
        (Some("cursor-cli".to_string()), "agent".to_string())
    );
}

#[tokio::test]
async fn a_conversation_child_of_a_cursor_cli_session_starts_a_cursor_cli_child() {
    // Given — a cursor-cli grill-me orchestrator on a host with the built-in coding agents
    let orchestrator = an_orchestrator_of("cursor-cli", "grill-me", HostAgents::TheBuiltins);

    // When — its agent spawns a conversation, naming no agent
    let child = orchestrator
        .conversation_spawn_handler()
        .spawn_conversation("Implement the token store.", Some("token-store"), None)
        .await
        .expect("the conversation spawns");

    // Then — the conversation is a cursor-cli session, and Cursor's binary is what ran
    assert_eq!(
        (
            orchestrator.session_type_of(&child),
            orchestrator.the_agent_launched().await,
        ),
        (Some("cursor-cli".to_string()), "agent".to_string())
    );
}

#[tokio::test]
async fn a_host_without_coding_agents_refuses_a_pr_stack_child_failed_precondition_spawning_nothing(
) {
    // Given — a cursor-cli pr-stack orchestrator on a host given no coding agents, whose daemon
    // config still names a claude binary
    let orchestrator = an_orchestrator_of("cursor-cli", "pr-stack", HostAgents::None);

    // When
    let refused = orchestrator
        .child_spawn_handler()
        .spawn_child(NODE_ID)
        .await;

    // Then — refused naming the missing coding agents; no child session, no agent launched
    assert_eq!(
        (
            refused,
            orchestrator.other_sessions(),
            orchestrator.agents_launched_so_far(),
        ),
        (
            Err(
                "this host has no coding agents installed: a cursor-cli session is launched from \
                 its tool definition"
                    .to_string()
            ),
            Vec::<String>::new(),
            String::new(),
        )
    );
}
