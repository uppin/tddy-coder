//! Acceptance: sandboxed `opencode-cli` and `muse-cli` sessions (`StartSession.sandbox = true`) on a
//! provider account held in the owner's vault.
//!
//! The daemon is the real `DaemonSessionHost` with its sandbox RPC bridge installed, and the jail is
//! the real one — Seatbelt on macOS, the cgroups backend on Linux — running the real
//! `tddy-sandbox-runner` with `--agent-kind opencode | muse`. The harness binary is a stub script
//! that prints its argv and the route's credentials as the jail resolved them, then waits on stdin,
//! so the terminal stream shows exactly what the jailed harness was started with.
//!
//! Every jail a test starts is terminated when the test ends, pass or fail, so no runner outlives
//! its test.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md
//! Feature: docs/ft/daemon/muse-cli-session.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_core::session_metadata::read_session_metadata;
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    ResumeSessionRequest, SessionService as SessionServiceTrait, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_terminal_rpc::proto::terminal_session::{
    StreamReplayMode, StreamTerminalOutputRequest,
    TerminalSessionService as TerminalSessionServiceTrait,
};

const VALID_TOKEN: &str = "valid-token";
const OWNER: &str = "testuser";
const PASSPHRASE: &str = "correct horse battery staple";
const TEST_PROJECT_ID: &str = "sandboxed-cli-harness-project";
const META_ACCOUNT: &str = "meta-hackathon";
const META_KEY: &str = "a-meta-key-for-the-jail";
const MUSE_MODEL: &str = "muse-wonder-1.0";

/// How long the jailed stub has to print its argv on the terminal stream.
const TERMINAL_OBSERVE: Duration = Duration::from_secs(60);

/// A daemon with one registered project, whose `session_type` harness binary is a stub, and whose
/// owner holds a Meta account in an unlocked vault.
struct SandboxedHarnessDaemon {
    service: Arc<DaemonSessionHost>,
    sessions_base: PathBuf,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _vault_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
}

/// Terminates a started session's jail when dropped, so a failing assertion leaves no runner behind.
struct JailGuard(Option<u32>);

impl Drop for JailGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process(pid);
        }
    }
}

fn a_sandboxed_daemon_for(session_type: &str, stub_name: &str) -> SandboxedHarnessDaemon {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    register_project(&sessions_dir.path().join("projects"), repo_dir.path());
    let stub = a_stub_harness_printing_its_launch(repo_dir.path(), stub_name);
    let (config_dir, config) = daemon_config_with_cli_harness_binary(session_type, &stub);
    let vault_dir = tempfile::tempdir().unwrap();
    let sessions_base = sessions_dir.path().to_path_buf();
    let service = Arc::new(
        session_host(config, sessions_base.clone())
            .with_credential_vaults(vaults_holding_a_meta_account(vault_dir.path())),
    );
    service.install_sandbox_rpc_bridge();
    SandboxedHarnessDaemon {
        service,
        sessions_base,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _vault_dir: vault_dir,
        _config_dir: config_dir,
    }
}

impl SandboxedHarnessDaemon {
    /// Start `req`, and guard the jail it started.
    async fn start(&self, req: StartSessionRequest) -> (String, JailGuard) {
        let session_id = self
            .service
            .start_session(Request::direct(req))
            .await
            .expect("a sandboxed start on a provider account succeeds")
            .into_inner()
            .session_id;
        let pid = read_session_metadata(&self.session_dir(&session_id))
            .ok()
            .and_then(|meta| meta.pid);
        (session_id, JailGuard(pid))
    }

    /// Resume `session_id`, and guard the jail its resume relaunched.
    async fn resume(&self, session_id: &str) -> JailGuard {
        self.service
            .resume_session(Request::direct(ResumeSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("a sandboxed session on a provider account resumes");
        JailGuard(
            read_session_metadata(&self.session_dir(session_id))
                .ok()
                .and_then(|meta| meta.pid),
        )
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// The session's terminal output until it shows every one of `wanted`, or what it showed by
    /// [`TERMINAL_OBSERVE`].
    async fn terminal_showing(&self, session_id: &str, wanted: &[&str]) -> String {
        let mut stream = self
            .service
            .terminal_session_service()
            .stream_terminal_output(Request::direct(StreamTerminalOutputRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                terminal_id: String::new(),
                initial_cols: 200,
                initial_rows: 24,
                mode: StreamReplayMode::Tail as i32,
                from_offset: 0,
                control_token: String::new(),
            }))
            .await
            .expect("the terminal stream opens")
            .into_inner();
        let mut seen = String::new();
        let _ = tokio::time::timeout(TERMINAL_OBSERVE, async {
            while let Some(Ok(msg)) = stream.next().await {
                seen.push_str(&String::from_utf8_lossy(&msg.data));
                if wanted.iter().all(|w| seen.contains(w)) {
                    return;
                }
            }
        })
        .await;
        seen
    }
}

fn a_sandboxed_start_on_the_meta_account(session_type: &str, model: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: session_type.to_string(),
        model: model.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        provider_account_id: META_ACCOUNT.to_string(),
        sandbox: true,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

/// **a_sandboxed_opencode_session_runs_in_its_jail_on_the_provider_account**: the runner hosts
/// OpenCode in its PTY on the route's model, with the route's configuration file and the key
/// resolved from the jail's secret file.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn a_sandboxed_opencode_session_runs_in_its_jail_on_the_provider_account() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    // Given
    let daemon = a_sandboxed_daemon_for("opencode-cli", "stub_opencode.sh");

    // When
    let (session_id, _jail) = daemon
        .start(a_sandboxed_start_on_the_meta_account(
            "opencode-cli",
            MUSE_MODEL,
        ))
        .await;

    // Then the session is recorded as a sandboxed OpenCode session
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).expect("metadata");
    assert_eq!(meta.session_type.as_deref(), Some("opencode-cli"));
    assert_eq!(meta.sandbox, Some(true));

    // And the jailed harness runs on the route's model, its config file and its key
    let terminal = daemon
        .terminal_showing(
            &session_id,
            &["ARGV:", "OPENCODE_CONFIG=/", &format!("KEY={META_KEY}")],
        )
        .await;
    assert!(terminal.contains("ARGV:"), "no harness output: {terminal}");
    assert!(
        terminal.contains(&format!("--model tddy-route/{MUSE_MODEL}")),
        "{terminal}"
    );
    assert!(terminal.contains("OPENCODE_CONFIG=/"), "{terminal}");
    assert!(terminal.contains(&format!("KEY={META_KEY}")), "{terminal}");
}

// ---------------------------------------------------------------------------
// Muse Code
// ---------------------------------------------------------------------------

/// **a_sandboxed_muse_session_runs_in_its_jail_on_the_provider_account**: the runner hosts Muse
/// Code in its PTY on the session's model, with the Meta key resolved from the jail's secret file.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn a_sandboxed_muse_session_runs_in_its_jail_on_the_provider_account() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    // Given
    let daemon = a_sandboxed_daemon_for("muse-cli", "stub_muse.sh");

    // When
    let (session_id, _jail) = daemon
        .start(a_sandboxed_start_on_the_meta_account(
            "muse-cli", MUSE_MODEL,
        ))
        .await;

    // Then the session is recorded as a sandboxed Muse session
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).expect("metadata");
    assert_eq!(meta.session_type.as_deref(), Some("muse-cli"));
    assert_eq!(meta.sandbox, Some(true));

    // And the jailed harness runs on the session's model and the account's key
    let terminal = daemon
        .terminal_showing(&session_id, &["ARGV:", &format!("META={META_KEY}")])
        .await;
    assert!(terminal.contains(MUSE_MODEL), "{terminal}");
    assert!(terminal.contains(&format!("META={META_KEY}")), "{terminal}");
}

/// **resuming_a_sandboxed_muse_session_relaunches_its_jail_continuing_on_the_account**: a resume
/// relaunches the session's jail — never the host PTY — with the harness continuing its session, on
/// the account its start recorded.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn resuming_a_sandboxed_muse_session_relaunches_its_jail_continuing_on_the_account() {
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    // Given a sandboxed Muse session on the Meta account
    let daemon = a_sandboxed_daemon_for("muse-cli", "stub_muse.sh");
    let (session_id, _started_jail) = daemon
        .start(a_sandboxed_start_on_the_meta_account(
            "muse-cli", MUSE_MODEL,
        ))
        .await;

    // When it is resumed
    let _resumed_jail = daemon.resume(&session_id).await;

    // Then the relaunched jail continues Muse's session on the account's key
    let meta = read_session_metadata(&daemon.session_dir(&session_id)).expect("metadata");
    assert_eq!(meta.sandbox, Some(true));
    assert_eq!(meta.status, "active");
    let terminal = daemon
        .terminal_showing(&session_id, &["--continue", &format!("META={META_KEY}")])
        .await;
    assert!(terminal.contains("--continue"), "{terminal}");
    assert!(terminal.contains(&format!("META={META_KEY}")), "{terminal}");
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A stub harness that prints its argv, `OPENCODE_CONFIG`, OpenCode's route key and Muse's
/// `META_API_KEY` on its terminal, then waits on stdin so the session stays up.
fn a_stub_harness_printing_its_launch(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo \"ARGV: $@\"\n\
         echo \"OPENCODE_CONFIG=$OPENCODE_CONFIG\"\n\
         echo \"KEY=$TDDY_OPENCODE_PROVIDER_KEY\"\n\
         echo \"META=$META_API_KEY\"\n\
         cat\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The vaults of a daemon on which the owner has unlocked a vault holding a Meta account.
fn vaults_holding_a_meta_account(dir: &Path) -> Arc<SessionVaults> {
    let passphrase = SecretString::new(PASSPHRASE);
    let store = CredentialStore::create(&CredentialStore::path_in(dir, OWNER), &passphrase, OWNER)
        .expect("the owner's vault is created");
    let meta = preset("meta").expect("the meta preset");
    let account = ProviderAccount {
        account: AccountId::new(META_ACCOUNT),
        preset: meta,
        label: "Meta hackathon".to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new(META_KEY),
    };
    store
        .put(account.to_record(1_760_000_000))
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir));
    vaults
        .unlock(OWNER, &passphrase)
        .expect("the passphrase opens the vault");
    vaults
}

fn daemon_config_with_cli_harness_binary(
    session_type: &str,
    binary: &Path,
) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "{OWNER}"
    os_user: "{OWNER}"
allowed_tools:
  - path: /bin/true
    label: true
cli_harnesses:
  {session_type}:
    binary_path: {binary}
"#,
        binary = binary.display()
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn session_host(config: DaemonConfig, sessions_base: PathBuf) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_data_dir,
        Arc::new(|token| (token == VALID_TOKEN).then(|| OWNER.to_string())),
        None,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(ClaudeCliSessionManager::new()),
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
        "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: sandboxed-cli-harness\n    git_url: \"\"\n    main_repo_path: {}\n",
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}
