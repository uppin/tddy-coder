//! Acceptance tests: Claude Code CLI session type (PRD: docs/ft/daemon/claude-cli-session.md).
//!
//! These tests define the desired behaviour of `session_type = "claude-cli"` sessions:
//! session metadata persistence, enrichment without changeset.yaml, LiveKit-free responses,
//! and resume in the existing worktree.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_metadata::{read_session_metadata, write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Code, Request};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

use tddy_service::proto::session::{
    ListSessionsRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";

/// What the old argv builder produced, now rendered from the `claude` YAML spec's `run.argv`
/// with the production binding (blank values trimmed away, as `harness_template_params`
/// binds them): binary, model, session, permissions, prompt.
#[allow(clippy::too_many_arguments)]
fn spec_claude_argv(
    binary: &str,
    model: &str,
    session_id: &str,
    prompt: Option<&str>,
    permission_mode: Option<&str>,
    dangerously_skip_permissions: bool,
    resume: bool,
) -> Vec<String> {
    let mut flags: Vec<&str> = Vec::new();
    if resume {
        flags.push("resume");
    }
    if dangerously_skip_permissions {
        flags.push("dangerously_skip_permissions");
    }
    tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached()
        .get("claude")
        .expect("the claude spec loads")
        .render_run_argv(
            std::path::Path::new(binary),
            &[
                ("model", model.trim()),
                ("session_id", session_id),
                (
                    "permission_mode",
                    permission_mode.map(str::trim).unwrap_or(""),
                ),
                ("prompt", prompt.map(str::trim).unwrap_or("")),
            ],
            &flags,
        )
        .expect("the claude argv renders")
}

/// The launch argv a `claude` session runs, rendered from its YAML spec with the stub as its
/// binary: the manager spawns what it is given, so the argv these tests assert on comes from
/// the spec (`packages/tddy-coding-agent-specs/specs/claude.yaml`), as production renders it.
fn a_claude_argv(
    stub: &std::path::Path,
    session_id: &str,
    permission_mode: Option<&str>,
    resume: bool,
) -> Vec<String> {
    let mut flags: Vec<&str> = Vec::new();
    if resume {
        flags.push("resume");
    }
    tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached()
        .get("claude")
        .expect("the claude spec loads")
        .render_run_argv(
            stub,
            &[
                ("model", TEST_MODEL),
                ("session_id", session_id),
                ("permission_mode", permission_mode.unwrap_or("")),
            ],
            &flags,
        )
        .expect("the claude argv renders")
}

const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";
// Safety-net ceiling, not an expected duration: `wait_for_capture_contains` returns the instant
// the stub's `ARGV:` line appears (~0.3s locally), so this only guards against false failures when
// a real PTY subprocess spawn + relay is starved under parallel-test CPU load. Matches the 10s
// RPC-stub ceiling below.

/// The OS user the test process runs as — a real, resolvable user (same-user, so the interactive
/// claude-cli spawn needs no privilege drop). Fixtures use this rather than a fabricated name so
/// impersonation resolves during the spawn.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A daemon config, and the coding agents the host installs: `claude` is the stub.
type AConfigAndItsAgents = (
    DaemonConfig,
    Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
);

fn write_config_with_claude_cli_binary(
    stub_binary: &str,
) -> (tempfile::TempDir, AConfigAndItsAgents) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[(
        "claude",
        std::path::Path::new(stub_binary),
    )]);
    (dir, (config, agents))
}

fn minimal_service(config: AConfigAndItsAgents, sessions_base: PathBuf) -> DaemonSessionHost {
    minimal_service_with_manager(
        config,
        sessions_base,
        Arc::new(tddy_session_lifecycle::claude_cli_session::CliSessionManager::new()),
    )
}

fn minimal_service_with_manager(
    (config, agents): AConfigAndItsAgents,
    sessions_base: PathBuf,
    manager: Arc<tddy_session_lifecycle::claude_cli_session::CliSessionManager>,
) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver = Arc::new(move |token| {
        if token == VALID_TOKEN {
            Some(resolved_user.clone())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        manager,
    )
    .with_coding_agents_if_none(agents)
}

/// Create a git repo with an origin remote pointing at itself so that
/// `git fetch origin` / `setup_worktree_for_session_with_optional_chain_base` succeed.
fn create_test_repo_with_origin(dir: &std::path::Path) {
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
    // Add the repo itself as origin so git fetch origin works without a real server.
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

/// Write a `projects.yaml` in `projects_dir` registering the given repo as TEST_PROJECT_ID.
fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// Write an executable shell script that echoes all CLI arguments.
/// Used as a stub for `claude` in PTY tests — avoids the `/bin/cat` trap (a positional arg
/// makes `cat` open it as a file rather than printing it).
fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_claude.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"ARGV: $@\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

/// **claude_cli_session_metadata_fields_persisted**: after `StartSession` with
/// `session_type = "claude-cli"` succeeds, `.session.yaml` under the new session directory must
/// contain `session_type = "claude-cli"` and `model = TEST_MODEL`. The `repo_path` must point to
/// a real linked git worktree of the project repo (visible in `git worktree list`).
#[tokio::test]
async fn claude_cli_session_metadata_fields_persisted() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    // `/bin/cat` as a stub for `claude` — works in a PTY without the real binary.
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(), // default: new_branch_from_base with generated name
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect("StartSession with session_type=claude-cli must succeed");

    // Then
    let session_id = resp.into_inner().session_id;
    assert!(!session_id.is_empty(), "session_id must be non-empty");

    // sessions_base_resolver returns sessions_tmp.path() directly (no username segment);
    // the daemon appends SESSIONS_SUBDIR ("sessions") and session_id.
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be written for claude-cli session");

    assert_eq!(
        meta.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must be persisted as 'claude-cli'"
    );
    assert_eq!(
        meta.model.as_deref(),
        Some(TEST_MODEL),
        "model must be persisted in .session.yaml"
    );
    assert!(
        meta.repo_path.is_some(),
        "repo_path must be set to the created worktree path"
    );
    let worktree_path = PathBuf::from(meta.repo_path.unwrap());
    assert!(
        worktree_path.exists(),
        "worktree directory must exist at repo_path: {}",
        worktree_path.display()
    );

    // Assert it is a real git worktree (appears in `git worktree list` for the project repo).
    let wt_list = std::process::Command::new("git")
        .args(["worktree", "list"])
        .current_dir(repo_dir.path())
        .output()
        .expect("git worktree list must run");
    let wt_stdout = String::from_utf8_lossy(&wt_list.stdout);
    assert!(
        wt_stdout
            .lines()
            .any(|l| l.starts_with(worktree_path.to_str().unwrap())),
        "worktree must appear in 'git worktree list' for the project repo;\n\
         worktree_path={}\ngit worktree list:\n{}",
        worktree_path.display(),
        wt_stdout
    );
}

/// **claude_cli_session_livekit_fields_empty**: `StartSessionResponse` for
/// `session_type = "claude-cli"` must return empty `livekit_room`, `livekit_url`, and
/// `livekit_server_identity` — no LiveKit room is created for these sessions.
#[tokio::test]
async fn claude_cli_session_livekit_fields_empty() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When
    let inner = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect("StartSession must succeed")
        .into_inner();

    // Then
    assert!(
        inner.livekit_room.is_empty(),
        "livekit_room must be empty for claude-cli sessions; got: {}",
        inner.livekit_room
    );
    assert!(
        inner.livekit_url.is_empty(),
        "livekit_url must be empty for claude-cli sessions; got: {}",
        inner.livekit_url
    );
    assert!(
        inner.livekit_server_identity.is_empty(),
        "livekit_server_identity must be empty for claude-cli sessions; got: {}",
        inner.livekit_server_identity
    );
}

/// **claude_cli_session_enrichment_reads_from_metadata**: when a session directory contains
/// `.session.yaml` with `session_type = "claude-cli"` and no `changeset.yaml`, `ListSessions`
/// must return `agent = "claude-cli"` and `model` from the metadata — not placeholder dashes.
#[tokio::test]
async fn claude_cli_session_enrichment_reads_from_metadata() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000001";
    let session_dir = sessions_tmp
        .path()
        .join(current_os_user())
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Write a .session.yaml with session_type and model.
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(99999),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
        tmux_session: None,
    };
    write_session_metadata(&session_dir, &meta).unwrap();
    // No changeset.yaml — intentionally absent to test the claude-cli fallback path.

    let user = current_os_user();
    let config_yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
"#
    );
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[(
        "claude",
        std::path::Path::new("/bin/cat"),
    )]);

    let sessions_base = sessions_tmp.path().join(current_os_user());
    let service = minimal_service((config, agents), sessions_base);

    // When
    let sessions = service
        .list_sessions(Request::direct(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions must succeed")
        .into_inner()
        .sessions;

    // Then
    let entry = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must appear in ListSessions response");

    assert_eq!(
        entry.agent, "claude-cli",
        "agent must be 'claude-cli' for claude-cli sessions (not '—')"
    );
    assert_eq!(
        entry.model, TEST_MODEL,
        "model must be read from .session.yaml for claude-cli sessions (not '—')"
    );
    assert!(
        entry.workflow_goal.is_empty() || entry.workflow_goal == "—",
        "workflow_goal should be empty/dash for claude-cli sessions (no changeset)"
    );
    assert!(
        entry.workflow_state.is_empty() || entry.workflow_state == "—",
        "workflow_state should be empty/dash for claude-cli sessions (no changeset)"
    );
}

/// **claude_cli_session_resume_relaunches_in_worktree**: after a claude-cli session becomes
/// inactive (process exits), `ResumeSession` must relaunch `claude --model <model>` in the
/// existing worktree and mark the session active again.
#[tokio::test]
async fn claude_cli_session_resume_relaunches_in_worktree() {
    // Given
    let worktree_dir = tempfile::tempdir().unwrap();
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000002";
    let session_dir = sessions_tmp
        .path()
        .join(current_os_user())
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Simulate an inactive claude-cli session (process previously exited).
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree_dir.path().to_str().unwrap().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
        tmux_session: None,
    };
    write_session_metadata(&session_dir, &meta).unwrap();

    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let sessions_base = sessions_tmp.path().join(current_os_user());
    let service = minimal_service(config, sessions_base);

    // When
    let resp = service
        .resume_session(Request::direct(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("ResumeSession must succeed for an inactive claude-cli session");

    // Then
    let inner = resp.into_inner();
    assert_eq!(
        inner.session_id, session_id,
        "ResumeSession must return the same session_id"
    );
    assert!(
        inner.livekit_room.is_empty(),
        "resumed claude-cli session must not return a livekit_room"
    );

    // After resume, metadata must be updated to active with a fresh PID.
    let updated_meta =
        read_session_metadata(&session_dir).expect(".session.yaml must be readable after resume");
    assert_eq!(
        updated_meta.status, "active",
        "session must be marked active after resume"
    );
    assert!(
        updated_meta.pid.is_some(),
        "session must have a non-None pid after resume (new PTY process spawned)"
    );
}

/// **claude_cli_start_session_requires_model**: `StartSession` with `session_type = "claude-cli"`
/// and an empty `model` must return `INVALID_ARGUMENT`.
#[tokio::test]
async fn claude_cli_start_session_requires_model() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let config_yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
"#
    );
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();
    let agents = tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[(
        "claude",
        std::path::Path::new("/bin/cat"),
    )]);
    let service = minimal_service((config, agents), sessions_tmp.path().to_path_buf());

    // When
    let err = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: String::new(), // empty — must be rejected before project lookup
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect_err("StartSession with claude-cli and empty model must fail");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "empty model for claude-cli must yield INVALID_ARGUMENT"
    );
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("model"),
        "error message must mention 'model'; got: {}",
        err.message
    );
}

/// **claude_cli_start_session_requires_project**: `StartSession` with `session_type = "claude-cli"`
/// and an empty `project_id` must return `INVALID_ARGUMENT`.
#[tokio::test]
async fn claude_cli_start_session_requires_project() {
    // Given
    // Empty projects dir so find_project returns None cleanly.
    let sessions_tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(sessions_tmp.path().join("projects")).unwrap();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    // When — Empty project_id → InvalidArgument.
    let err = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: String::new(), // empty
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect_err("StartSession with empty project_id must fail");

    // Then
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "empty project_id for claude-cli must yield INVALID_ARGUMENT"
    );

    // When — Unknown project_id → NotFound.
    let err2 = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: "no-such-project".to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: String::new(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect_err("StartSession with unknown project_id must fail");

    // Then
    assert_eq!(
        err2.code,
        Code::NotFound,
        "unknown project_id for claude-cli must yield NOT_FOUND"
    );
}

/// A daemon host over `sessions_base` that was given **no** coding agents — the host a daemon
/// whose `<tddyhome>/tools` registry failed to load runs as. Nothing else differs from
/// [`minimal_service`].
fn a_host_without_coding_agents(config: DaemonConfig, sessions_base: PathBuf) -> DaemonSessionHost {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: UserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
    DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
    )
}

/// A host start of `session_type` on a registered project, naming its branch.
fn a_cli_start_of(session_type: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: session_type.to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: format!("{session_type}-work"),
        ..Default::default()
    }
}

/// A machine with no `claude` anywhere: a `$HOME` whose `.local/bin` is empty, one `PATH`
/// directory that does not hold it, and `tddy-tools` named by `TDDY_TOOLS_BINARY`. Nothing of the
/// real machine — whose `~/.local/bin/claude` a developer's laptop has — reaches resolution.
struct AMachineWithoutClaude {
    home: PathBuf,
    path_dir: PathBuf,
    tddy_tools: PathBuf,
    _dir: tempfile::TempDir,
}

fn a_machine_without_claude() -> AMachineWithoutClaude {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    let path_dir = root.join("usr/local/bin");
    std::fs::create_dir_all(&path_dir).unwrap();
    let tddy_tools = root.join("install/tddy-tools");
    std::fs::create_dir_all(tddy_tools.parent().unwrap()).unwrap();
    std::fs::write(&tddy_tools, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&tddy_tools, std::fs::Permissions::from_mode(0o755)).unwrap();
    AMachineWithoutClaude {
        home,
        path_dir,
        tddy_tools,
        _dir: dir,
    }
}

impl tddy_actions::tool::ToolHost for AMachineWithoutClaude {
    fn env_var(&self, name: &str) -> Option<String> {
        match name {
            "HOME" => Some(self.home.to_string_lossy().into_owned()),
            "TDDY_TOOLS_BINARY" => Some(self.tddy_tools.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![self.path_dir.clone()]
    }

    fn is_executable_file(&self, path: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &std::path::Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A daemon.yaml with no agent section, as `DaemonConfig::load` reads one at start-up.
fn a_config_without_agent_sections() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!("users:\n  - github_user: \"{user}\"\n    os_user: \"{user}\"\n"),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must load");
    (dir, config)
}

/// **a_claude_cli_start_whose_binary_is_not_found_…** (`#agent-tools 19/19`): the daemon's host
/// finds `claude` through `TDDY_CLAUDE_BINARY`, then `$HOME/.local/bin`, then `PATH` — and when
/// none holds it, the start is refused naming the agent, its spec's `binary.env` and every place
/// searched. There is no daemon.yaml `claude_cli.binary_path` to fall back to, and no "as-is" bare
/// `claude` handed to the spawn.
#[tokio::test]
async fn a_claude_cli_start_whose_binary_is_not_found_is_refused_naming_its_binary_env() {
    // Given — a registered project, a daemon.yaml with no agent section, and no claude anywhere
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    let sessions_base = sessions_tmp.path().join(current_os_user());
    register_project(&sessions_base.join("projects"), repo_dir.path());
    let machine = Arc::new(a_machine_without_claude());
    let (_cfg_dir, config) = a_config_without_agent_sections();
    let registry = tddy_coding_agent_specs::load_agent_registry(&sessions_base.join("tools"))
        .expect("the built-in agents load");
    let host = a_host_without_coding_agents(config, sessions_base.clone()).with_coding_agents(
        Arc::new(tddy_agent_launch::spec_launch::HostCodingAgents::new(
            registry,
            tddy_agent_launch::host_coding_agents::daemon_tool_host(machine.clone()),
        )),
    );

    // When
    let started = host
        .start_session(Request::direct(a_cli_start_of("claude-cli")))
        .await
        .map(|response| response.into_inner().session_id);

    // Then — refused, naming claude, TDDY_CLAUDE_BINARY and the host's search path
    assert_eq!(
        started.map_err(|status| (status.code, status.message)),
        Err((
            Code::FailedPrecondition,
            format!(
                "tool claude is unavailable: searched TDDY_CLAUDE_BINARY (unset), PATH ({}:{})",
                machine.home.join(".local/bin").display(),
                machine.path_dir.display(),
            ),
        ))
    );
}

// ---------------------------------------------------------------------------
// initial_prompt tests
// ---------------------------------------------------------------------------

/// **build_claude_argv_includes_positional_prompt_when_present**: the pure argv-builder appends
/// the initial prompt as the last positional argument when non-empty.
#[test]
fn build_claude_argv_includes_positional_prompt_when_present() {
    // When / Then
    let argv = spec_claude_argv(
        "/usr/local/bin/claude",
        "claude-opus-4-8",
        "test-session-id",
        Some("build a hello world app"),
        None,
        false,
        false,
    );

    assert_eq!(
        argv,
        vec![
            "/usr/local/bin/claude",
            "--model",
            "claude-opus-4-8",
            "--session-id",
            "test-session-id",
            "--permission-mode",
            "auto",
            "build a hello world app",
        ],
        "argv must be: binary --model <m> --session-id <id> --permission-mode auto <prompt>"
    );
}

/// **build_claude_argv_omits_when_empty_or_none**: `None`, `Some("")`, and `Some("   ")` must
/// all produce the same argv without a trailing positional argument.
#[test]
fn build_claude_argv_omits_when_empty_or_none() {
    // Given
    let expected = vec![
        "/usr/local/bin/claude".to_string(),
        "--model".to_string(),
        "claude-opus-4-8".to_string(),
        "--session-id".to_string(),
        "sid".to_string(),
        "--permission-mode".to_string(),
        "auto".to_string(),
    ];

    // When / Then
    assert_eq!(
        spec_claude_argv(
            "/usr/local/bin/claude",
            "claude-opus-4-8",
            "sid",
            None,
            None,
            false,
            false
        ),
        expected,
        "None initial_prompt must produce no positional arg"
    );
    assert_eq!(
        spec_claude_argv(
            "/usr/local/bin/claude",
            "claude-opus-4-8",
            "sid",
            Some(""),
            None,
            false,
            false
        ),
        expected,
        "empty string must produce no positional arg"
    );
    assert_eq!(
        spec_claude_argv(
            "/usr/local/bin/claude",
            "claude-opus-4-8",
            "sid",
            Some("   "),
            None,
            false,
            false
        ),
        expected,
        "whitespace-only must produce no positional arg (trimmed to empty)"
    );
}

/// **claude_cli_session_passes_initial_prompt_as_positional_arg**: `manager.start(.., Some("…"))`
/// results in the prompt appearing in the child process's `$@`.
#[tokio::test]
async fn claude_cli_session_passes_initial_prompt_as_positional_arg() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = CliSessionManager::new();

    // When
    let handle = manager
        .start_with_options(
            "test-session-with-prompt",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            spec_claude_argv(
                stub_path.to_str().unwrap(),
                TEST_MODEL,
                "test-session-with-prompt",
                Some("build a hello world app"),
                None,
                false,
                false,
            ),
            "claude-cli",
            Some("build a hello world app"),
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("start with echo-argv stub and initial_prompt must succeed");

    // Then
    let output = a_capture_showing(&handle.capture, "ARGV:", PTY_STUB_OUTPUT).await;

    assert!(
        output.contains("build a hello world app"),
        "initial_prompt must appear in ARGV output; got: {:?}",
        output
    );
    assert!(
        output.contains("--session-id"),
        "--session-id must appear in ARGV output; got: {:?}",
        output
    );
}

/// **claude_cli_session_empty_prompt_adds_no_positional_arg**: `Some("")` produces the same argv
/// as `None` — no empty positional arg appended.
#[tokio::test]
async fn claude_cli_session_empty_prompt_adds_no_positional_arg() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = CliSessionManager::new();

    // When
    let handle = manager
        .start_with_options(
            "test-session-empty-prompt",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            spec_claude_argv(
                stub_path.to_str().unwrap(),
                TEST_MODEL,
                "test-session-empty-prompt",
                Some(""),
                None,
                false,
                false,
            ),
            "claude-cli",
            Some(""), // empty — must not add a stray positional arg
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("start with empty initial_prompt must succeed");

    // Then
    let output = a_capture_showing(&handle.capture, "ARGV:", PTY_STUB_OUTPUT).await;

    let argv_line = output
        .lines()
        .find(|l| l.trim_start().starts_with("ARGV:"))
        .unwrap_or("");

    let parts: Vec<&str> = argv_line.split_whitespace().collect();
    assert!(
        !parts.is_empty(),
        "ARGV line must not be empty; full output: {:?}",
        output
    );
    // Without a prompt, the last element must be "auto" (the --permission-mode value), not an empty string.
    assert_eq!(
        parts.last().copied(),
        Some("auto"),
        "last ARGV element must be 'auto' (permission-mode default) when prompt is empty; ARGV line: {:?}",
        argv_line
    );
}

/// **start_session_claude_cli_threads_initial_prompt_from_request**: the `StartSession` RPC
/// threads `initial_prompt` down to the PTY process; the shared manager registry holds the
/// session (proving it is attachable via terminal-stream RPCs).
#[tokio::test]
async fn start_session_claude_cli_threads_initial_prompt_from_request() {
    // Given
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());

    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let (_cfg_dir, config) = write_config_with_claude_cli_binary(stub_path.to_str().unwrap());

    let shared_manager = Arc::new(CliSessionManager::new());
    let service = minimal_service_with_manager(
        config,
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&shared_manager),
    );

    // When
    let resp = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: String::new(),
            new_branch_name: String::new(),
            selected_integration_base_ref: String::new(),
            selected_branch_to_work_on: String::new(),
            initial_prompt: "hello from rpc".to_string(),
            permission_mode: String::new(),
            stack_parent: String::new(),
            sandbox: false,
            managed_codebase: false,
            specialized_agents: vec![],
            ..Default::default()
        }))
        .await
        .expect("StartSession with initial_prompt must succeed");

    // Then
    let session_id = resp.into_inner().session_id;
    assert!(!session_id.is_empty(), "session_id must be non-empty");

    // The shared manager must hold the session — proves attachability via terminal-stream RPCs.
    let handle = shared_manager
        .get(&session_id)
        .await
        .expect("session must be present in the shared CliSessionManager after start");

    let output = a_capture_showing(&handle.capture, "ARGV:", PTY_STUB_OUTPUT).await;

    assert!(
        output.contains("hello from rpc"),
        "initial_prompt from StartSession RPC must appear in ARGV output; got: {:?}",
        output
    );
}

/// **resume_does_not_replay_initial_prompt**: `manager.resume()` always passes `initial_prompt =
/// None`; the resumed process must start without a seeded prompt (re-injecting it would create a
/// duplicate user turn in the claude session history).
#[tokio::test]
async fn resume_does_not_replay_initial_prompt() {
    // Given
    let stub_dir = tempfile::tempdir().unwrap();
    let stub_path = write_echo_argv_script(stub_dir.path());

    let worktree_dir = tempfile::tempdir().unwrap();
    let manager = CliSessionManager::new();

    // Initial start — seeds a prompt.
    let _handle1 = manager
        .start_with_options(
            "test-session-resume-noreplay",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            spec_claude_argv(
                stub_path.to_str().unwrap(),
                TEST_MODEL,
                "test-session-resume-noreplay",
                Some("original prompt"),
                None,
                false,
                false,
            ),
            "claude-cli",
            Some("original prompt"),
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("initial start must succeed");

    // When — must NOT replay the initial_prompt.
    let handle2 = manager
        .resume(
            "test-session-resume-noreplay",
            worktree_dir.path().to_path_buf(),
            "claude-opus-4-8",
            a_claude_argv(&stub_path, "test-session-resume-noreplay", None, true),
            "claude-cli",
        )
        .await
        .expect("resume must succeed");

    // Then
    let output = a_capture_showing(&handle2.capture, "ARGV:", PTY_STUB_OUTPUT).await;

    let argv_line = output
        .lines()
        .find(|l| l.trim_start().starts_with("ARGV:"))
        .unwrap_or("");

    assert!(
        !argv_line.contains("original prompt"),
        "resume must NOT replay the initial_prompt; ARGV line: {:?}",
        argv_line
    );
    assert!(
        argv_line.contains("--resume"),
        "--resume must be present in resumed session ARGV; ARGV line: {:?}",
        argv_line
    );
    assert!(
        !argv_line.contains("--session-id"),
        "a resumed session must use --resume, not --session-id; ARGV line: {:?}",
        argv_line
    );
}
