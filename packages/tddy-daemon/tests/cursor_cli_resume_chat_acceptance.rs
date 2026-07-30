//! Acceptance tests: a resumed cursor-cli session must reattach to its original Cursor chat.
//!
//! Regression context — session `019fa71b-8463-7b33-98a5-f07bb830c803`: `ResumeSession` spawned
//! `cursor-agent --model <model>` with no chat selector, so the Cursor CLI opened a brand new
//! chat and the user lost the whole conversation. Cursor's own on-disk store recorded the two
//! chats side by side under the session's worktree hash: `88147fb1-…` created at the session's
//! `created_at`, and a fresh `3186e3a9-…` created one second after the resume RPC.
//!
//! The contract these tests pin:
//!
//! 1. A cursor-cli session owns exactly one Cursor chat id for its whole lifetime.
//! 2. That id is minted up front (`cursor-agent create-chat`, which prints a bare id on stdout)
//!    and persisted in `.session.yaml` as `cursor_chat_id`, so it survives a daemon restart.
//! 3. Every spawn for that session — the first one and every resume — passes
//!    `--resume <cursor_chat_id>`, keeping the agent in the same chat.
//!
//! `--resume` takes an *optional* argument (`--resume [chatId]`), so a bare `--resume` makes the
//! CLI open an interactive chat picker that would wedge the PTY. The argv assertions therefore
//! check the flag and its id as an adjacent pair, never just that `--resume` is present.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon::claude_cli_session::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ResumeSessionRequest, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "glm-5.2-high";
const TEST_PROJECT_ID: &str = "cursor-resume-project";

/// The chat id the stub `cursor-agent create-chat` mints. A real `create-chat` prints a bare
/// chat id on stdout and exits 0 — verified against cursor-agent 2026.07.23.
const MINTED_CHAT_ID: &str = "f8db82db-e154-41d0-ae72-312bdf6d4d80";

/// The Cursor chat that session `019fa71b-…` was really talking to before the bad resume.
const ORIGINAL_CHAT_ID: &str = "88147fb1-bdb6-43d9-94d8-3c9b7da4d806";

/// The session whose resume opened a new chat instead of reattaching.
const INCIDENT_SESSION_ID: &str = "019fa71b-8463-7b33-98a5-f07bb830c803";

/// How long a spawned stub gets to record its argv before we call it a failure.
const SPAWN_OBSERVE_TIMEOUT_MS: u64 = 3_000;

// ---------------------------------------------------------------------------
// Stub cursor-agent — a fake binary, not a mock: it serves `create-chat` from a
// fixed id and records every invocation so the test can assert on real argv.
// ---------------------------------------------------------------------------

/// A stand-in `cursor-agent` that answers `create-chat` and logs what it was called with.
struct StubCursorAgent {
    binary_path: PathBuf,
    argv_log: PathBuf,
    create_chat_log: PathBuf,
}

/// Write a stub `cursor-agent` into `dir` that mints `MINTED_CHAT_ID` for `create-chat`.
///
/// Paths and the minted id are baked into the script text rather than passed through the
/// environment, so the stub behaves identically no matter how the daemon spawns it.
fn a_stub_cursor_agent(dir: &Path) -> StubCursorAgent {
    let binary_path = dir.join("stub_cursor_agent.sh");
    let argv_log = dir.join("argv.log");
    let create_chat_log = dir.join("create_chat.log");
    let script = format!(
        r#"#!/bin/sh
if [ "$1" = "create-chat" ]; then
  echo "create-chat" >> "{create_chat_log}"
  echo "{MINTED_CHAT_ID}"
  exit 0
fi
printf '%s' "$1" >> "{argv_log}"
shift
for arg in "$@"; do printf '\t%s' "$arg" >> "{argv_log}"; done
printf '\n' >> "{argv_log}"
cat
"#,
        create_chat_log = create_chat_log.display(),
        argv_log = argv_log.display(),
    );
    std::fs::write(&binary_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    StubCursorAgent {
        binary_path,
        argv_log,
        create_chat_log,
    }
}

impl StubCursorAgent {
    /// The args of the most recent agent spawn, waiting up to `SPAWN_OBSERVE_TIMEOUT_MS` for it.
    ///
    /// Excludes argv[0] (a shell script never sees its own path in `"$@"`).
    async fn last_spawn_args(&self) -> Vec<String> {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(SPAWN_OBSERVE_TIMEOUT_MS);
        while std::time::Instant::now() < deadline {
            let logged = std::fs::read_to_string(&self.argv_log).unwrap_or_default();
            if let Some(last) = logged.lines().last().filter(|l| !l.is_empty()) {
                return last.split('\t').map(str::to_string).collect();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!(
            "stub cursor-agent was never spawned: {} recorded no argv within {SPAWN_OBSERVE_TIMEOUT_MS}ms",
            self.argv_log.display()
        );
    }

    /// How many times `cursor-agent create-chat` ran.
    fn create_chat_invocations(&self) -> usize {
        std::fs::read_to_string(&self.create_chat_log)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }
}

// ---------------------------------------------------------------------------
// Argv assertions
// ---------------------------------------------------------------------------

trait CursorArgvAssertions {
    fn assert_resumes_chat(&self, chat_id: &str) -> &Self;
    fn assert_runs_model(&self, model: &str) -> &Self;
    fn assert_carries_no_prompt(&self) -> &Self;
}

impl CursorArgvAssertions for Vec<String> {
    /// `--resume <chat_id>` must appear as an adjacent pair — a bare `--resume` opens an
    /// interactive chat picker that would wedge the PTY.
    fn assert_resumes_chat(&self, chat_id: &str) -> &Self {
        let resumed = self
            .windows(2)
            .find(|pair| pair[0] == "--resume")
            .map(|pair| pair[1].as_str());
        assert_eq!(
            resumed,
            Some(chat_id),
            "cursor-agent must be launched with `--resume {chat_id}` so it reattaches to the \
             session's existing chat; got argv {self:?}"
        );
        self
    }

    fn assert_runs_model(&self, model: &str) -> &Self {
        let selected = self
            .windows(2)
            .find(|pair| pair[0] == "--model")
            .map(|pair| pair[1].as_str());
        assert_eq!(
            selected,
            Some(model),
            "cursor-agent must keep the session's model; got argv {self:?}"
        );
        self
    }

    /// A resume must not replay a prompt — every non-flag token has to be a flag's value.
    fn assert_carries_no_prompt(&self) -> &Self {
        let flags_taking_a_value = ["--model", "--resume"];
        let positionals: Vec<&String> = self
            .iter()
            .enumerate()
            .filter(|(i, arg)| {
                !arg.starts_with("--")
                    && !i
                        .checked_sub(1)
                        .and_then(|prev| self.get(prev))
                        .is_some_and(|prev| flags_taking_a_value.contains(&prev.as_str()))
            })
            .map(|(_, arg)| arg)
            .collect();
        assert!(
            positionals.is_empty(),
            "a resumed cursor-cli session must not replay a positional prompt, \
             found {positionals:?} in argv {self:?}"
        );
        self
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// A daemon with one registered project and a stub `cursor-agent`.
struct CursorCliDaemon {
    service: ConnectionServiceImpl,
    manager: Arc<CliSessionManager>,
    agent: StubCursorAgent,
    sessions_base: PathBuf,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_cursor_cli_daemon() -> CursorCliDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    register_project(&sessions_dir.path().join("projects"), repo_dir.path());
    let stub_dir = tempfile::tempdir().unwrap();
    let agent = a_stub_cursor_agent(stub_dir.path());

    let config = daemon_config_with_cursor_binary(agent.binary_path.to_str().unwrap());
    let sessions_base = sessions_dir.path().to_path_buf();
    let manager = Arc::new(CliSessionManager::new());
    let service = connection_service(config, sessions_base.clone(), Arc::clone(&manager));

    CursorCliDaemon {
        service,
        manager,
        agent,
        sessions_base,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

impl CursorCliDaemon {
    /// Start a cursor-cli session on a fresh branch and return its id.
    async fn start_a_cursor_cli_session(&self) -> String {
        let response = self
            .service
            .start_session(Request::new(a_cursor_cli_start_request()))
            .await
            .expect("StartSession cursor-cli must succeed");
        response.into_inner().session_id
    }

    /// Seed a stopped cursor-cli session directory the way an earlier daemon run left it.
    ///
    /// The `.session.yaml` is written as raw YAML so the test states the on-disk contract
    /// directly: `cursor_chat_id` is the key a restarted daemon has to read the chat back from.
    fn a_stopped_session_owning_chat(&self, session_id: &str, chat_id: Option<&str>) -> PathBuf {
        let session_dir = self.sessions_base.join("sessions").join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let worktree = self.sessions_base.join("worktrees").join(session_id);
        std::fs::create_dir_all(&worktree).unwrap();

        let chat_line = chat_id
            .map(|id| format!("cursor_chat_id: {id}\n"))
            .unwrap_or_default();
        let yaml = format!(
            "session_id: {session_id}\n\
             project_id: {TEST_PROJECT_ID}\n\
             created_at: 2026-07-28T05:03:46Z\n\
             updated_at: 2026-07-28T05:03:46Z\n\
             status: stopped\n\
             repo_path: {worktree}\n\
             session_type: cursor-cli\n\
             model: {TEST_MODEL}\n\
             {chat_line}",
            worktree = worktree.display(),
        );
        std::fs::write(session_dir.join(".session.yaml"), yaml).unwrap();
        session_dir
    }

    async fn resume(&self, session_id: &str) {
        self.service
            .resume_session(Request::new(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect(
                "ResumeSession cursor-cli must succeed — a `session not found` here means \
                 SessionMetadata still rejects the `cursor_chat_id` key in .session.yaml \
                 (the struct is #[serde(deny_unknown_fields)])",
            );
    }

    /// The chat id `.session.yaml` records for `session_id`, read as raw YAML.
    fn recorded_chat_id(&self, session_id: &str) -> Option<String> {
        let path = self
            .sessions_base
            .join("sessions")
            .join(session_id)
            .join(".session.yaml");
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        let doc: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();
        doc.get("cursor_chat_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// Wait until the session has a live PTY, so a spawn failure fails here and not in an assertion.
    async fn assert_agent_is_running(&self, session_id: &str) {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(SPAWN_OBSERVE_TIMEOUT_MS);
        while std::time::Instant::now() < deadline {
            if self.manager.get(session_id).await.is_some() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("session {session_id} never registered a PTY in CliSessionManager");
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn a_cursor_cli_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "cursor-cli".to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        ..Default::default()
    }
}

fn daemon_config_with_cursor_binary(binary_path: &str) -> DaemonConfig {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
cursor_cli:
  binary_path: {binary_path}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    DaemonConfig::load(&config_path).expect("config must parse")
}

fn connection_service(
    config: DaemonConfig,
    sessions_base: PathBuf,
    manager: Arc<CliSessionManager>,
) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        manager,
    )
}

fn create_test_repo_with_origin(dir: &Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: cursor-resume-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

// ---------------------------------------------------------------------------
// Start: mint a chat and pin it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn starting_a_cursor_cli_session_records_the_chat_id_it_will_resume_into() {
    // Given
    let daemon = a_cursor_cli_daemon();

    // When
    let session_id = daemon.start_a_cursor_cli_session().await;

    // Then
    assert_eq!(
        daemon.recorded_chat_id(&session_id).as_deref(),
        Some(MINTED_CHAT_ID),
        ".session.yaml must record cursor_chat_id at start so a later resume can reattach \
         to the same Cursor chat after a daemon restart"
    );
}

#[tokio::test]
async fn starting_a_cursor_cli_session_launches_the_agent_in_the_chat_it_recorded() {
    // Given
    let daemon = a_cursor_cli_daemon();

    // When
    let session_id = daemon.start_a_cursor_cli_session().await;

    // Then
    daemon.assert_agent_is_running(&session_id).await;
    daemon
        .agent
        .last_spawn_args()
        .await
        .assert_resumes_chat(MINTED_CHAT_ID)
        .assert_runs_model(TEST_MODEL);
}

// ---------------------------------------------------------------------------
// Resume: reattach instead of starting over — the reported bug
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resuming_a_cursor_cli_session_reattaches_to_its_original_chat() {
    // Given — the incident session as an earlier daemon run left it on disk
    let daemon = a_cursor_cli_daemon();
    daemon.a_stopped_session_owning_chat(INCIDENT_SESSION_ID, Some(ORIGINAL_CHAT_ID));

    // When
    daemon.resume(INCIDENT_SESSION_ID).await;

    // Then
    daemon.assert_agent_is_running(INCIDENT_SESSION_ID).await;
    daemon
        .agent
        .last_spawn_args()
        .await
        .assert_resumes_chat(ORIGINAL_CHAT_ID)
        .assert_runs_model(TEST_MODEL);
}

#[tokio::test]
async fn resuming_a_cursor_cli_session_does_not_mint_a_second_chat() {
    // Given
    let daemon = a_cursor_cli_daemon();
    daemon.a_stopped_session_owning_chat(INCIDENT_SESSION_ID, Some(ORIGINAL_CHAT_ID));

    // When
    daemon.resume(INCIDENT_SESSION_ID).await;
    daemon.assert_agent_is_running(INCIDENT_SESSION_ID).await;

    // Then
    assert_eq!(
        daemon.agent.create_chat_invocations(),
        0,
        "a session that already owns a chat must be resumed into it, not given a new one"
    );
}

#[tokio::test]
async fn resuming_a_cursor_cli_session_keeps_the_recorded_chat_id_unchanged() {
    // Given
    let daemon = a_cursor_cli_daemon();
    daemon.a_stopped_session_owning_chat(INCIDENT_SESSION_ID, Some(ORIGINAL_CHAT_ID));

    // When
    daemon.resume(INCIDENT_SESSION_ID).await;

    // Then
    assert_eq!(
        daemon.recorded_chat_id(INCIDENT_SESSION_ID).as_deref(),
        Some(ORIGINAL_CHAT_ID),
        "resume rewrites .session.yaml (pid, status, updated_at) and must carry the \
         session's chat id through untouched"
    );
}

#[tokio::test]
async fn resuming_a_cursor_cli_session_does_not_replay_the_initial_prompt() {
    // Given
    let daemon = a_cursor_cli_daemon();
    daemon.a_stopped_session_owning_chat(INCIDENT_SESSION_ID, Some(ORIGINAL_CHAT_ID));

    // When
    daemon.resume(INCIDENT_SESSION_ID).await;
    daemon.assert_agent_is_running(INCIDENT_SESSION_ID).await;

    // Then
    daemon
        .agent
        .last_spawn_args()
        .await
        .assert_carries_no_prompt();
}

// ---------------------------------------------------------------------------
// Resume: sessions started before chat ids were recorded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resuming_a_session_started_before_chat_ids_were_recorded_adopts_a_chat_for_it() {
    // Given — a legacy `.session.yaml` with no cursor_chat_id: its original chat is
    // unrecoverable, but the session must not stay unresumable forever.
    let daemon = a_cursor_cli_daemon();
    let legacy_session_id = "019f8d1f-283e-7590-a282-c971a4c3e018";
    daemon.a_stopped_session_owning_chat(legacy_session_id, None);

    // When
    daemon.resume(legacy_session_id).await;

    // Then
    assert_eq!(
        daemon.recorded_chat_id(legacy_session_id).as_deref(),
        Some(MINTED_CHAT_ID),
        "a legacy session must adopt a chat id on its first resume so every later resume \
         reattaches instead of starting over again"
    );
}

#[tokio::test]
async fn resuming_a_session_started_before_chat_ids_were_recorded_pins_the_adopted_chat() {
    // Given
    let daemon = a_cursor_cli_daemon();
    let legacy_session_id = "019f8d1f-283e-7590-a282-c971a4c3e018";
    daemon.a_stopped_session_owning_chat(legacy_session_id, None);

    // When
    daemon.resume(legacy_session_id).await;

    // Then — the adopted id is passed as `--resume <id>`; a bare `--resume` would drop the
    // agent into an interactive chat picker and wedge the PTY.
    daemon.assert_agent_is_running(legacy_session_id).await;
    daemon
        .agent
        .last_spawn_args()
        .await
        .assert_resumes_chat(MINTED_CHAT_ID);
}
