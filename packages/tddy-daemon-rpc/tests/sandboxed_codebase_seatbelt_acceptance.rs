//! Acceptance: a **jailed-codebase** session's tools inside a real Seatbelt jail.
//!
//! `sandboxed_codebase_placement_acceptance.rs` proves the daemon *routes* a jailed-codebase
//! session's tool calls into a jail. That is the daemon's claim. Whether the jail then holds is
//! the kernel's claim, and only a real one can answer it: these tests start a genuine
//! jailed-codebase session through the production provisioner and ask it for files it must not be
//! able to reach.
//!
//! The distinction this suite exists for: confinement must be the **kernel's** refusal, not the
//! tool engine's own path checks. `tddy-tool-engine`'s `contain_path` rejects anything resolving
//! outside the worktree root before a syscall happens, so a `Read` test cannot tell a jail from no
//! jail — it is named for what it actually pins. The kernel's claim is carried by the jailed
//! `Shell`, which has no such guard in front of it and which first proves, in the same test, that
//! it can read a file *inside* the checkout: otherwise a jail simply missing `cat` would pass.
//!
//! PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
//! Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md
//!
//! Linux: the cgroups jail shares the host filesystem root — the minimal read-only root with
//! `pivot_root` is unbuilt (docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md) — so the filesystem
//! assertions below would fail there for a reason that is documented rather than accidental. The
//! Linux half of this claim belongs in `tddy-e2e`'s VM-backed suite, beside
//! `vm_workspace_tool_sandbox_acceptance.rs`.

use std::path::{Path, PathBuf};

use std::sync::{Arc, OnceLock};
use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_daemon_sandbox::workspace_tool_sandbox::RUNNER_PID_FILE;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145b77";

/// The deployment secret this daemon signs its session tokens with — what lets it mint the agent
/// a credential of its own for the tool calls it makes back here. Without it the start is refused
/// rather than forwarding the caller's token, so the fixture below would never reach a jail.
const LK_API_SECRET: &str = "secret";

/// Long enough for a jailed `sh -c` to finish, short enough that a wedged jail fails the test
/// rather than hanging the suite.
const SHELL_BLOCK_MS: u64 = 30_000;

/// What a host file outside the checkout contains. A jail that can read it leaks this exact string.
const HOST_SECRET: &str = "a-host-file-the-jailed-codebase-must-never-read";

/// A file seeded *inside* the checkout, and what it holds. The positive control for the jailed
/// shell: a `cat` that cannot read this one is a jail with no `cat`, not a jail that confines.
const INSIDE_FILE: &str = "inside-the-checkout.txt";
const INSIDE_CONTENT: &str = "readable-from-inside-the-jail";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn sandbox_runner_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// The OS user this test process runs as. The shared `test_service` config maps to a fabricated
/// `testdev`, which does not resolve during the claude-cli spawn — and unlike the workspace-only
/// suites next door, this one actually completes one.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// The credential the browser presents, signed with [`LK_API_SECRET`] so this daemon can verify it
/// and mint the agent's own from the identity it proves. Minted once and shared, because the
/// request and the daemon's user resolver have to agree on the very same string.
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

/// A daemon on the production jail provisioner, mapped to a user that exists, with the agent
/// spawn stubbed. The jail is real; only the agent binary is not, because these tests are about
/// what the kernel refuses, not about what Claude does.
fn a_daemon(sessions_base: std::path::PathBuf) -> TestDaemon {
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
claude_cli:
  binary_path: /bin/cat
livekit:
  api_secret: "{LK_API_SECRET}"
"#
    );
    let config: DaemonConfig = serde_yaml::from_str(&yaml).expect("config must parse");
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let user_resolver: SessionUserResolver = Arc::new(move |token| {
        if token == a_caller_token() {
            Some(current_os_user())
        } else {
            None
        }
    });
    TestDaemon::from_host(DaemonSessionHost::new(
        config,
        resolver,
        sessions_base,
        user_resolver,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    ))
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

fn a_git_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_projects::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_projects::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "sandboxed-codebase-seatbelt".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

struct ShellResult {
    stdout: String,
    exit_code: i32,
}

/// A jailed-codebase session on a real daemon, plus a host file deliberately left outside its
/// checkout for the jail to fail to reach.
struct AJailedCodebase {
    service: TestDaemon,
    /// The workspace session holding the checkout — the id the agent's MCP addresses, and the one
    /// whose tools run in the jail.
    checkout_session_id: String,
    worktree: PathBuf,
    host_secret_file: PathBuf,
    /// The `tddy-sandbox-runner` this fixture started, as the jail itself recorded it. Read at
    /// construction rather than at teardown because the session directory it lives under is one of
    /// the tempdirs below, and those are gone by the time [`Drop`] runs.
    runner_pid: u32,
    _host_outside_dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

/// Tear the jail down on every exit from the test, a failed assertion included.
///
/// The pattern `in_jail_conversation_acceptance.rs`'s `BridgedJail` already uses, for the same
/// reason: a `tddy-sandbox-runner` is a real child process put in its own process group, so it is
/// reparented to the init process when this binary exits and killing the test never reaps it — a
/// panic that skipped teardown leaves it on the host for as long as the machine is up.
///
/// The signal goes first and the reaping is left to the daemon: field drop order runs this body
/// before `service`, whose `WorkspaceSandboxRegistry` then `kill`s (a no-op on an already-dying
/// process) and `wait`s for the very `Child` it owns. Killing without reaping and then reaping
/// without racing is exactly what `terminate_sandbox_process` does on the same handle.
impl Drop for AJailedCodebase {
    fn drop(&mut self) {
        // SAFETY: `kill(2)` with a pid this daemon spawned and still owns; no memory is touched.
        unsafe { libc::kill(self.runner_pid as libc::pid_t, libc::SIGKILL) };
    }
}

async fn a_jailed_codebase_session() -> AJailedCodebase {
    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(
        runner.exists(),
        "build tddy-sandbox-runner first: {}",
        runner.display()
    );
    assert!(
        tools.exists(),
        "build tddy-tools first: {}",
        tools.display()
    );

    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    register_project(sessions.path(), repo.path());
    let service = a_daemon(sessions.path().to_path_buf());

    // A host file that is emphatically not in the checkout, and not under the session directory
    // either — the two trees the jail legitimately holds.
    let host_outside_dir = tempfile::tempdir().expect("host tempdir");
    let host_secret_file = host_outside_dir.path().join("host-secret.txt");
    std::fs::write(&host_secret_file, HOST_SECRET).expect("write host secret");

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: a_caller_token().to_string(),
            session_type: "claude-cli".to_string(),
            project_id: PROJECT_ID.to_string(),
            model: "claude-opus-5".to_string(),
            sandboxed_codebase: true,
            ..Default::default()
        }))
        .await
        .expect("a jailed-codebase session must start on a host with Seatbelt")
        .into_inner();

    let agent_dir = unified_session_dir_path(sessions.path(), &started.session_id);
    let agent = tddy_core::read_session_metadata(&agent_dir).expect("agent session metadata");
    let checkout_session_id = agent
        .codebase_session_id
        .expect("the agent half must record the workspace session holding its checkout");

    let checkout_dir = unified_session_dir_path(sessions.path(), &checkout_session_id);
    let checkout = tddy_core::read_session_metadata(&checkout_dir).expect("checkout metadata");

    let checkout_is_jailed = checkout.sandbox;
    let agent_is_jailed = agent.sandbox;
    let worktree = PathBuf::from(checkout.repo_path.expect("checkout worktree"));
    let runner_pid = recorded_runner_pid(&checkout_dir);

    // Assembled *before* the premise is asserted, because the jail is already running by now: a
    // premise that fails has to unwind through something that owns the runner, or it orphans the
    // very jail it is complaining about.
    let jailed = AJailedCodebase {
        service,
        checkout_session_id,
        worktree,
        host_secret_file,
        runner_pid,
        _host_outside_dir: host_outside_dir,
        _repo: repo,
        _sessions: sessions,
    };

    // The fixture's own premise, asserted before any test builds on it. A checkout that came up
    // unsandboxed would still serve every tool below, from the bare host, and each assertion after
    // this point would be about the tool engine rather than about a jail.
    assert_eq!(
        checkout_is_jailed,
        Some(true),
        "the checkout must actually be jailed for anything below to be about the jail"
    );
    // The other half of the premise, and the one that distinguishes this placement from `sandbox`:
    // the agent itself is not confined.
    assert_eq!(
        agent_is_jailed, None,
        "the agent must be unconfined — jailing it is the placement this one inverts"
    );

    jailed
}

/// The runner pid the jail under `checkout_dir` recorded for itself.
///
/// Read from the jail's own pid file rather than guessed, so the reaper names exactly the process
/// the daemon holds — and `expect`ed rather than defaulted, because a fixture that cannot say
/// which process it started cannot promise to stop it either.
fn recorded_runner_pid(checkout_dir: &Path) -> u32 {
    let pid_file = checkout_dir.join("sandbox").join(RUNNER_PID_FILE);
    let raw = std::fs::read_to_string(&pid_file)
        .unwrap_or_else(|e| panic!("the jail must record its runner pid at {pid_file:?}: {e}"));
    raw.trim()
        .parse()
        .unwrap_or_else(|e| panic!("{pid_file:?} must hold a pid, got {raw:?}: {e}"))
}

impl AJailedCodebase {
    async fn execute_tool(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::new(ExecuteToolRequest {
                session_token: a_caller_token().to_string(),
                session_id: self.checkout_session_id.clone(),
                daemon_instance_id: String::new(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
            }))
            .await
            .expect("ExecuteTool must not fail at the RPC level")
            .into_inner()
    }

    /// Run `command` in the jail and return its stdout and exit code.
    async fn shell(&self, command: &str) -> ShellResult {
        let response = self
            .execute_tool(
                "Shell",
                serde_json::json!({ "command": command, "block_until_ms": SHELL_BLOCK_MS }),
            )
            .await;
        assert!(
            !response.is_error,
            "the Shell call itself must reach the jail; error was '{}'",
            response.error_message
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&response.result_json).unwrap_or_else(|e| {
                panic!("Shell result must be JSON ({e}): {}", response.result_json)
            });
        // `expect`, not a default: a Shell result that stopped carrying these fields would make
        // every confinement assertion below pass on an empty stdout and a fabricated exit code.
        ShellResult {
            stdout: parsed["stdout"]
                .as_str()
                .unwrap_or_else(|| panic!("Shell result must carry a string stdout: {parsed}"))
                .to_string(),
            exit_code: parsed["exit_code"]
                .as_i64()
                .unwrap_or_else(|| panic!("Shell result must carry a numeric exit_code: {parsed}"))
                as i32,
        }
    }
}

// ---------------------------------------------------------------------------
// Confinement
// ---------------------------------------------------------------------------

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "the Linux cgroups jail shares the host filesystem root (pivot_root unbuilt, \
              docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md), so filesystem confinement is a \
              macOS claim today; the Linux half belongs in tddy-e2e's VM-backed suite"
)]
#[tokio::test]
async fn a_write_from_a_jailed_codebase_session_lands_inside_the_checkout() {
    // Given a jailed-codebase session
    let jailed = a_jailed_codebase_session().await;

    // When the agent writes a file through the jail
    let response = jailed
        .execute_tool(
            "Write",
            serde_json::json!({ "path": "notes.md", "contents": "written through the jail" }),
        )
        .await;

    // Then it landed in the checkout on the host, at the path the agent named
    assert!(
        !response.is_error,
        "a write inside the checkout must succeed; error was '{}'",
        response.error_message
    );
    let written = std::fs::read_to_string(jailed.worktree.join("notes.md"))
        .expect("the jail mounts the checkout read-write, so the host sees the file");
    assert_eq!(written, "written through the jail");
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "the Linux cgroups jail shares the host filesystem root (pivot_root unbuilt, \
              docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md), so filesystem confinement is a \
              macOS claim today; the Linux half belongs in tddy-e2e's VM-backed suite"
)]
#[tokio::test]
async fn a_shell_from_a_jailed_codebase_session_runs_with_the_checkout_as_its_cwd() {
    // Given a jailed-codebase session
    let jailed = a_jailed_codebase_session().await;

    // When the agent asks the jail where it is
    let result = jailed.shell("pwd").await;

    // Then it is standing in the checkout, at the same path the host resolved — a path that meant
    // two different things on the two sides of the jail would make every tool argument ambiguous
    assert_eq!(result.exit_code, 0);
    assert_eq!(
        result.stdout.trim(),
        jailed.worktree.display().to_string(),
        "the checkout is mounted at its own host path, so both sides name it identically"
    );
}

/// Deliberately **not** named "by the jail". `tddy-tool-engine`'s `contain_path` rejects any path
/// that canonicalises outside the worktree root before a syscall is ever attempted, so this test
/// would pass identically with no jail at all and cannot carry the kernel's claim. It is still
/// worth having: it pins that the containment check runs on the in-jail engine too, where the
/// worktree root is the mounted checkout. The kernel's refusal is
/// [`a_shell_cannot_reach_outside_the_checkout_either`], which has no such guard in front of it.
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "the Linux cgroups jail shares the host filesystem root (pivot_root unbuilt, \
              docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md), so filesystem confinement is a \
              macOS claim today; the Linux half belongs in tddy-e2e's VM-backed suite"
)]
#[tokio::test]
async fn a_read_outside_the_checkout_is_refused_by_the_tool_engines_path_containment() {
    // Given a jailed-codebase session and a host file outside its checkout
    let jailed = a_jailed_codebase_session().await;
    let outside = jailed.host_secret_file.display().to_string();

    // When the agent reads that absolute path through the jail
    let response = jailed
        .execute_tool("Read", serde_json::json!({ "path": outside }))
        .await;

    // Then it is refused for escaping the worktree, and the secret does not come back
    assert!(
        response.is_error,
        "a read outside the checkout must fail, not return empty content"
    );
    assert!(
        response.error_message.contains("escapes worktree"),
        "the refusal must be the path-containment one, so a change of guard is visible here rather than silently reattributed to the jail; got '{}'",
        response.error_message
    );
    assert!(
        !response.result_json.contains(HOST_SECRET),
        "the jail leaked a host file it must not be able to open: {}",
        response.result_json
    );
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "the Linux cgroups jail shares the host filesystem root (pivot_root unbuilt, \
              docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md), so filesystem confinement is a \
              macOS claim today; the Linux half belongs in tddy-e2e's VM-backed suite"
)]
#[tokio::test]
async fn a_shell_cannot_reach_outside_the_checkout_either() {
    // Given a jailed-codebase session whose shell demonstrably *can* cat a file inside the
    // checkout. Without this control the test passes on a jail where `cat` is simply missing,
    // which is confinement of the tool rather than of the filesystem.
    let jailed = a_jailed_codebase_session().await;
    std::fs::write(jailed.worktree.join(INSIDE_FILE), INSIDE_CONTENT)
        .expect("the host can seed a file in the checkout the jail holds");
    let control = jailed.shell(&format!("cat {INSIDE_FILE}")).await;
    assert_eq!(
        control.exit_code, 0,
        "the control must succeed, or the refusal below says nothing about the filesystem"
    );
    assert_eq!(control.stdout.trim(), INSIDE_CONTENT);
    let outside = jailed.host_secret_file.display().to_string();

    // When the agent tries to cat a host file outside it — the route a build would take
    let result = jailed.shell(&format!("cat {outside}")).await;

    // Then the jail refuses it. `Read` is the tool engine's path; this is the kernel's.
    assert_ne!(
        result.exit_code, 0,
        "the jail must refuse a shell reaching outside the checkout"
    );
    assert!(
        !result.stdout.contains(HOST_SECRET),
        "the jailed shell leaked a host file: {}",
        result.stdout
    );
}
