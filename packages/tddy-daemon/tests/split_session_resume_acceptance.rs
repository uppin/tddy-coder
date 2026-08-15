//! Acceptance: resuming a **split** session — the agent runs here, its worktree lives on another
//! daemon.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md` § Resume.
//!
//! Nothing about a split session's tool transport survives a stop. The `TDDY_REMOTE_*` environment
//! was injected into a process that has exited, the join token it carried is scoped to a TTL that
//! may have elapsed, and there is no `repo_path` on this host to fall back to. All of it has to be
//! re-derived on resume from the one durable part — the `codebase_daemon_instance_id` /
//! `codebase_session_id` pairing in `.session.yaml`.
//!
//! These need no LiveKit container: minting a join token reads the daemon's own config and joins
//! nothing, and the codebase daemon is never contacted by a resume. What is real here is the PTY
//! spawn, so the assertions read the environment of the process that was actually launched rather
//! than an intermediate the daemon could get right on its own and hand over wrong.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ResumeSessionRequest,
};
use tddy_testing_commons::stub_scripts::a_stub_agent_script;
use tddy_testing_commons::wait::eventually_blocking;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The deployment-wide secret in this daemon's `livekit:` block. It signs LiveKit room JWTs *and*
/// session tokens, which is what lets the codebase daemon verify a credential minted here.
const LK_API_SECRET: &str = "secret";
const MODEL: &str = "claude-opus-5";
const PROJECT_ID: &str = "split-resume-proj";
/// The session on this daemon — the one being resumed.
const AGENT_SESSION_ID: &str = "0199aaaa-0000-7000-8000-00000000000a";
/// The paired `workspace` session on the codebase daemon, whose worktree the agent works in.
const CODEBASE_SESSION_ID: &str = "0199bbbb-0000-7000-8000-00000000000b";
const CODEBASE_INSTANCE_ID: &str = "workstation-b";
/// This daemon — the one resuming the session, hosting its room, and running the agent.
const FACILITATING_INSTANCE_ID: &str = "laptop-a";
const COMMON_ROOM: &str = "tddy-lobby";
const LIVEKIT_URL: &str = "ws://livekit.invalid:7880";

/// Lifetime the daemon mints a split agent's join token with (`split_session::SPLIT_AGENT_TOKEN_TTL`).
/// Spelled out rather than imported so that shortening the production TTL fails these tests instead
/// of silently moving the expectation with it.
const EXPECTED_TOKEN_TTL: Duration = Duration::from_secs(86_400);

/// Ceiling on waiting for the spawned stub to record its environment, not an expected duration:
/// locally the file appears in well under a second, and this only guards a PTY spawn starved under
/// a parallel test run.
const STUB_RECORD_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The credential the browser presents on `ResumeSession`, signed with [`LK_API_SECRET`] so this
/// daemon can verify it. Minted once and shared, because the request and the daemon's user resolver
/// have to agree on the very same string, and the agent's own token is minted from *these* claims.
fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        SessionTokenSigner::new(LK_API_SECRET.as_bytes()).mint_access(&GitHubUser {
            id: 4242,
            login: current_os_user(),
            avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
            name: "Test User".to_string(),
        })
    })
}

/// The OS user the test process runs as: a real, resolvable user, so the claude-cli spawn needs no
/// privilege drop.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A stub `claude` that dumps its environment in one write and then holds its PTY open.
///
/// `env > tmp; mv -f tmp target` rather than a per-variable append: a reader polling the file sees
/// either nothing or the whole environment, never a half-written record that would look like the
/// daemon having built the wrong env.
fn a_claude_stub_recording_its_env(dir: &std::path::Path, env_file: &std::path::Path) -> PathBuf {
    let target = env_file.display();
    a_stub_agent_script(dir, "stub-claude.sh")
        .with_prelude(&format!(
            "env > \"{target}.tmp.$$\"\nmv -f \"{target}.tmp.$$\" \"{target}\""
        ))
        .then_reading_stdin()
        .build()
}

/// A daemon that can wire a split session: LiveKit credentials to mint the agent's join token, and
/// a spawnable `claude`.
fn a_daemon_config(claude_binary: &std::path::Path) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let claude_binary = claude_binary.display();
    let yaml = format!(
        r#"
daemon_instance_id: "{FACILITATING_INSTANCE_ID}"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {claude_binary}
livekit:
  url: {LIVEKIT_URL}
  api_key: devkey
  api_secret: {LK_API_SECRET}
  common_room: {COMMON_ROOM}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn a_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == a_caller_token()).then(|| resolved_user.clone()));
    ConnectionServiceImpl::new(
        config,
        resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// A stopped split session as `StartSession` left it: paired with a workspace session on another
/// daemon, and with **no** `repo_path`, because there is no repository on this host.
fn a_stopped_split_session() -> SessionMetadata {
    SessionMetadata {
        session_id: AGENT_SESSION_ID.to_string(),
        project_id: PROJECT_ID.to_string(),
        created_at: "2026-08-13T10:00:00Z".to_string(),
        updated_at: "2026-08-13T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: None,
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(MODEL.to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        specialized_agents: Vec::new(),
        codebase_daemon_instance_id: Some(CODEBASE_INSTANCE_ID.to_string()),
        codebase_session_id: Some(CODEBASE_SESSION_ID.to_string()),
    }
}

/// Everything a resume needs on disk, plus the file the relaunched agent will write its env to.
struct ResumedSplitSession {
    env_file: PathBuf,
    _sessions: tempfile::TempDir,
    _stubs: tempfile::TempDir,
    _config: tempfile::TempDir,
}

impl ResumedSplitSession {
    /// The environment of the process the daemon actually launched.
    fn agent_env(&self) -> Vec<(String, String)> {
        let path = self.env_file.clone();
        eventually_blocking(
            "the relaunched agent to record its environment",
            STUB_RECORD_TIMEOUT,
            move || {
                let recorded = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{} not readable yet: {e}", path.display()))?;
                Ok(recorded
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>())
            },
        )
    }

    fn agent_env_var(&self, name: &str) -> String {
        let env = self.agent_env();
        env.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                // Only the remote-tool half of the environment: the inherited shell environment is
                // hundreds of variables of noise around the handful this feature sets.
                let remote: Vec<&str> = env
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .filter(|k| k.starts_with("TDDY_REMOTE_"))
                    .collect();
                panic!("the relaunched agent had no {name}; its TDDY_REMOTE_* env was {remote:?}")
            })
    }
}

/// Resume the stopped split session described by [`a_stopped_split_session`].
async fn resume_a_split_session() -> ResumedSplitSession {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let stub_dir = tempfile::tempdir().unwrap();
    let env_file = stub_dir.path().join("agent-env.txt");
    let claude_stub = a_claude_stub_recording_its_env(stub_dir.path(), &env_file);

    let sessions_base = sessions_tmp.path().join(current_os_user());
    let session_dir = sessions_base.join("sessions").join(AGENT_SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    write_session_metadata(&session_dir, &a_stopped_split_session()).unwrap();

    let (config_dir, config) = a_daemon_config(&claude_stub);
    let service = a_service(config, sessions_base);

    service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: AGENT_SESSION_ID.to_string(),
        }))
        .await
        .expect("a stopped split session must resume");

    ResumedSplitSession {
        env_file,
        _sessions: sessions_tmp,
        _stubs: stub_dir,
        _config: config_dir,
    }
}

/// Seconds-since-epoch of a JWT's `exp` claim, read from the unverified payload.
fn token_expiry_epoch_secs(jwt: &str) -> i64 {
    use base64::Engine;
    let payload = jwt
        .split('.')
        .nth(1)
        .unwrap_or_else(|| panic!("expected a three-part JWT; got {jwt:?}"));
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .unwrap_or_else(|e| panic!("JWT payload must be base64url: {e}; got {payload:?}"));
    let claims: serde_json::Value = serde_json::from_slice(&decoded)
        .unwrap_or_else(|e| panic!("JWT payload must be JSON: {e}; payload was {payload:?}"));
    claims["exp"]
        .as_i64()
        .unwrap_or_else(|| panic!("JWT must carry an exp claim; claims were {claims}"))
}

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the epoch")
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_resumed_split_agent_is_pointed_at_the_codebase_hosts_session_not_its_own() {
    // Given a stopped split session paired with a workspace session on another daemon
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the codebase daemon resolves the worktree from its own sessions base by this id, so
    // the agent's own session id would resolve to nothing there. A co-located resume gets this
    // right by accident because both ids name the same thing; a split resume cannot.
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_SESSION_ID"),
        CODEBASE_SESSION_ID,
        "the resumed agent must address the workspace session on the codebase host"
    );
}

#[tokio::test]
async fn a_resumed_split_agent_is_re_wired_to_its_rooms_host_and_the_daemon_it_was_paired_with() {
    // Given a stopped split session whose only record of the pairing is its .session.yaml
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the transport is rebuilt from the persisted pairing. A resume that dropped it would
    // leave the agent with no route to a worktree at all, since there is no repository on this host
    // to fall back to.
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_SERVER_IDENTITY"),
        format!("daemon-{FACILITATING_INSTANCE_ID}"),
        "tool calls are addressed at the host of the room the agent rejoins — this daemon; the \
         codebase daemon is in no room and would never answer"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_DAEMON_INSTANCE_ID"),
        CODEBASE_INSTANCE_ID,
        "the resumed agent must name the daemon holding its codebase"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_ROOM"),
        tddy_daemon::session_room::session_room_name(AGENT_SESSION_ID),
        "a resumed agent rejoins its own session's room — the one this daemon hosts as the session's \
         facilitating daemon, not one named after the codebase session on the other host"
    );
    assert_eq!(
        resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_URL"),
        LIVEKIT_URL,
        "the tool transport dials the configured LiveKit server"
    );
}

#[tokio::test]
async fn a_resumed_split_agent_receives_a_join_token_minted_at_the_resume() {
    // Given a split session stopped long enough for its original token to matter
    // When it is resumed
    let resumed = resume_a_split_session().await;

    // Then — the token is good for a full lifetime measured from *this* moment, not whatever was
    // left of the one the original agent process carried. The persisted pairing is the only durable
    // part of a split session's wiring; the credential is minted afresh every spawn.
    let expiry = token_expiry_epoch_secs(&resumed.agent_env_var("TDDY_REMOTE_LIVEKIT_TOKEN"));
    let expected = now_epoch_secs() + EXPECTED_TOKEN_TTL.as_secs() as i64;
    // Wall-clock: the token was minted a moment before this line ran, so its expiry trails the
    // expectation by however long the resume took. 60s covers a starved PTY spawn.
    assert!(
        (expected - expiry) >= 0 && (expected - expiry) <= 60,
        "expected the resumed agent's token to expire around {expected} (now + {}s); it expires at {expiry}, {} seconds off",
        EXPECTED_TOKEN_TTL.as_secs(),
        expected - expiry
    );
}
