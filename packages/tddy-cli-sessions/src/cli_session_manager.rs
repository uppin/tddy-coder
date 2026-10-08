//! CLI session manager — spawns agent CLIs (`claude`, Cursor `agent`) in PTYs and plumbs I/O via
//! tokio channels. Shared by `claude-cli` and `cursor-cli` session types (`CliSessionManager`).
//!
//! Uses `portable-pty` so TUI agents see a controlling terminal (TTY). Without a PTY, the child
//! detects no TTY and exits immediately.
//!
//! Historical import path `claude_cli_session` re-exports this module for compatibility.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_task::{TaskId, TaskRegistry};
use tokio::sync::{broadcast, RwLock};

use tddy_pty::PtyRegistry;

/// Reserved terminal id for the original `claude` terminal of a session.
///
/// Every terminal in a session is identified; the `claude` terminal spawned at session start is
/// always addressable under this id, while started shell terminals receive fresh unique ids.
pub const MAIN_TERMINAL_ID: &str = "main";

mod pty_handle;
pub use pty_handle::*;

/// The outcome of a [`CliSessionManager::claim_control`] call.
///
/// `tddy-terminal-rpc`'s own type: it is what the `ClaimTerminalControl` handler answers with, and
/// two structurally identical enums would need a converter between them that could only ever get
/// the mapping wrong in one direction.
pub use tddy_terminal_rpc::service::ControlClaim as ClaimOutcome;

/// Per-session control lease snapshot.
#[derive(Debug, Clone)]
pub struct ControlLeaseInfo {
    pub control_token: String,
    pub holder_screen_id: String,
}

/// Broadcast payload emitted when a session's control lease changes.
///
/// `tddy-terminal-rpc`'s own type, for the same reason as [`ClaimOutcome`] — and here it also means
/// the lease broadcast can be handed straight to that crate's `TerminalControl` port, rather than
/// through a relay task that re-broadcasts every event into a second channel of the same shape.
pub use tddy_terminal_rpc::service::ControlChange as ControlChangeEvent;

/// Per-terminal metadata: maps a terminal id to its backing task.
#[derive(Debug, Clone)]
struct TerminalEntry {
    task_id: TaskId,
    worktree_path: PathBuf,
    model: String,
}

/// `session_id → (terminal_id → TerminalEntry)`.
type TerminalIndex = Arc<RwLock<HashMap<String, HashMap<String, TerminalEntry>>>>;

/// Per-session control leases: `session_id → ControlLeaseInfo`.
type ControlRegistry = Arc<RwLock<HashMap<String, ControlLeaseInfo>>>;

/// Per-session managed-workflow wiring (kept alive for the session's lifetime): `session_id → ManagedWorkflow`.
type ManagedWorkflowRegistry =
    Arc<RwLock<HashMap<String, crate::session_toolcall::ManagedWorkflow>>>;

/// Per-session LiveKit exposure of the terminal: `session_id → BridgedTerminal`.
type LiveKitTerminalRegistry = Arc<RwLock<HashMap<String, BridgedTerminal>>>;

/// A session's terminal as LiveKit clients see it: what it publishes about itself, and the
/// participant serving it once one has been put in the room.
struct BridgedTerminal {
    /// The `session` block this session publishes about itself. Spawn-time knowledge — nothing on
    /// disk records which planned stack node a child materialized — so it is kept from the start
    /// rather than re-derived when the bridge is finally wanted.
    metadata: tddy_core::session_participant_metadata::SessionParticipantMetadata,
    /// The participant bridging the PTY, once a LiveKit consumer has arrived. Its task ends when
    /// the room or the PTY does, so a finished handle is a bridge that is no longer there.
    participant: Option<tokio::task::JoinHandle<()>>,
}

/// Where a session's terminal is served to LiveKit clients.
///
/// Every field is a pure function of the deployment config and the session id, so the caller
/// derives them rather than the manager reading configuration it otherwise knows nothing about.
pub struct LiveKitTerminalAddress {
    pub url: String,
    pub room: String,
    pub api_key: String,
    pub api_secret: String,
    pub identity: String,
}

/// Manages PTY session tools via the shared [`TaskRegistry`] and per-session control leases.
pub struct CliSessionManager {
    task_registry: TaskRegistry,
    pty_registry: PtyRegistry,
    /// Maps `(session_id, terminal_id)` to the backing task.
    terminals: TerminalIndex,
    /// Exclusive control lease per session.
    control: ControlRegistry,
    /// Fan-out channel for control-change events. Subscribers call [`Self::subscribe_control`].
    control_tx: broadcast::Sender<ControlChangeEvent>,
    /// Managed-workflow wiring per session — its toolcall listener + controller must outlive the
    /// spawned process, so the manager owns it and drops it when the main terminal exits.
    managed_workflows: ManagedWorkflowRegistry,
    /// What each session publishes about itself when its terminal is bridged into LiveKit, and the
    /// participant doing it once one has been put there.
    ///
    /// An entry is written when a session that is *meant* to be drivable from another host starts;
    /// the participant is added when a LiveKit consumer first arrives. A session with no entry is
    /// one that is not exposed over LiveKit at all — which is why the entry, and not the presence
    /// of a PTY, is what decides whether anything is bridged.
    livekit_terminals: LiveKitTerminalRegistry,
}

/// Backward-compatible alias for [`CliSessionManager`].
pub type ClaudeCliSessionManager = CliSessionManager;

impl Default for CliSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CliSessionManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::with_task_registry(TaskRegistry::new())
    }

    /// Create a manager sharing the given [`TaskRegistry`] (used by `DaemonSessionHost`).
    pub fn with_task_registry(task_registry: TaskRegistry) -> Self {
        let (control_tx, _) = broadcast::channel(64);
        Self {
            task_registry,
            pty_registry: PtyRegistry::new(),
            terminals: Arc::new(RwLock::new(HashMap::new())),
            control: Arc::new(RwLock::new(HashMap::new())),
            control_tx,
            managed_workflows: Arc::new(RwLock::new(HashMap::new())),
            livekit_terminals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Shared task registry — PTY tools and fast tools use the same instance.
    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    // --- terminal control lease (single-screen mutex) ---
}

mod livekit_terminals;

mod control_lease;

mod terminals;

mod relaunch;

mod launch;

mod argv;

mod pty_spawn;

// ---------------------------------------------------------------------------
// Resize escape parsing
// ---------------------------------------------------------------------------

mod livekit_bridge;
pub use livekit_bridge::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// The token immediately following `flag` in an argv, or `None` if the flag is absent.
    fn value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1))
            .map(String::as_str)
    }

    fn contains_flag(argv: &[String], flag: &str) -> bool {
        argv.iter().any(|a| a == flag)
    }

    #[test]
    fn resuming_builds_the_argv_with_resume_not_session_id() {
        // Given — a resume of an existing session
        let resume = true;

        // When
        let argv = ClaudeCliSessionManager::build_claude_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            None,
            None,
            false,
            resume,
        );

        // Then — the id is passed to --resume and --session-id is absent
        assert_eq!(
            value_after(&argv, "--resume"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--session-id"));
    }

    #[test]
    fn a_fresh_start_builds_the_argv_with_session_id_not_resume() {
        // Given — a fresh (non-resume) start
        let resume = false;

        // When
        let argv = ClaudeCliSessionManager::build_claude_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            None,
            None,
            false,
            resume,
        );

        // Then — the id is assigned via --session-id and --resume is absent
        assert_eq!(
            value_after(&argv, "--session-id"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--resume"));
    }

    fn pid_is_alive(pid: u32) -> bool {
        // kill -0 checks existence without sending a signal; ESRCH means dead
        let ret = unsafe { libc::kill(pid as i32, 0) };
        ret == 0
    }

    fn wait_for_pid_to_die(pid: u32, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if !pid_is_alive(pid) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        !pid_is_alive(pid)
    }

    /// kill_all terminates a running PTY process registered under a session.
    ///
    /// AC: After kill_all() returns, the process is dead (kill -0 → ESRCH).
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_terminates_registered_pty_processes() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        let handle = manager
            .start_terminal(
                "kill-all-test-session-1",
                worktree.path().to_path_buf(),
                "/bin/sh",
            )
            .await
            .expect("start_terminal should spawn a PTY process");
        let pid = handle.pid;

        assert!(pid_is_alive(pid), "process should be alive before kill_all");

        manager.kill_all().await;

        assert!(
            wait_for_pid_to_die(pid, std::time::Duration::from_secs(5)),
            "process pid={pid} should be dead after kill_all"
        );
    }

    /// kill_all kills all sessions when multiple are tracked simultaneously.
    ///
    /// AC: Every PID in every registered session is dead after kill_all().
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_terminates_all_sessions() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");
        let mut pids = Vec::new();

        for i in 0..3 {
            let session_id = format!("kill-all-multi-{i}");
            let handle = manager
                .start_terminal(&session_id, worktree.path().to_path_buf(), "/bin/sh")
                .await
                .expect("start_terminal should succeed");
            pids.push(handle.pid);
        }

        for &pid in &pids {
            assert!(
                pid_is_alive(pid),
                "pid {pid} should be alive before kill_all"
            );
        }

        manager.kill_all().await;

        for &pid in &pids {
            assert!(
                wait_for_pid_to_die(pid, std::time::Duration::from_secs(5)),
                "pid {pid} should be dead after kill_all"
            );
        }
    }

    /// kill_all empties the registry so subsequent lookups return nothing.
    ///
    /// AC: list_terminals returns empty for all sessions after kill_all().
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_clears_the_registry() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        manager
            .start_terminal("registry-clear-a", worktree.path().to_path_buf(), "/bin/sh")
            .await
            .expect("start session a");
        manager
            .start_terminal("registry-clear-b", worktree.path().to_path_buf(), "/bin/sh")
            .await
            .expect("start session b");

        assert!(
            !manager.list_terminals("registry-clear-a").await.is_empty(),
            "session-a should have terminals before kill_all"
        );

        manager.kill_all().await;

        assert!(
            manager.list_terminals("registry-clear-a").await.is_empty(),
            "session-a should have no terminals after kill_all"
        );
        assert!(
            manager.list_terminals("registry-clear-b").await.is_empty(),
            "session-b should have no terminals after kill_all"
        );
    }

    /// kill_all on an empty manager is safe and does not panic.
    ///
    /// AC: No panic or error on a freshly constructed manager.
    #[tokio::test]
    async fn kill_all_is_safe_on_empty_manager() {
        let manager = ClaudeCliSessionManager::new();
        manager.kill_all().await; // must not panic
    }

    /// When the tool binary cannot be spawned, the caller must see the real reason (e.g. the
    /// `spawn failed: …` from portable_pty), not a generic PTY-plumbing message. Regression for
    /// the masked "PTY runtime did not signal ready" error that hid a missing-binary failure.
    #[tokio::test]
    #[cfg(unix)]
    async fn surfaces_the_underlying_reason_when_the_binary_cannot_be_spawned() {
        // Given
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When
        let result = manager
            .start_terminal(
                "spawn-failure-session",
                worktree.path().to_path_buf(),
                "/nonexistent/definitely-not-a-real-binary",
            )
            .await;

        // Then
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("spawning a missing binary must fail");
        assert!(
            message.contains("spawn failed"),
            "error should surface the real spawn failure, was: {message}"
        );
    }

    /// A session pinned to an OS user that cannot be resolved must fail loudly, naming the user —
    /// never silently spawn under the daemon's own identity (which would defeat multi-user
    /// isolation on a root daemon).
    #[tokio::test]
    #[cfg(unix)]
    async fn fails_to_start_when_the_os_user_cannot_be_resolved() {
        // Given a manager and a session pinned to a user that does not exist on this host
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When starting the session as that unresolvable user
        let result = manager
            .start_with_options(
                "unknown-os-user-session",
                worktree.path().to_path_buf(),
                "some-model",
                "/bin/sh",
                None,
                None,
                false,
                false,
                None,
                Vec::new(),
                Vec::new(),
                Some("nyxzzz-nonexistent-user"),
            )
            .await;

        // Then the start fails and the error names the unresolvable user
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("start must fail for an unresolvable os_user");
        assert!(
            message.contains("nyxzzz-nonexistent-user"),
            "error should name the unresolvable os_user, was: {message}"
        );
    }

    /// The generic "did not signal ready" plumbing message must never stand in for a concrete
    /// spawn failure — otherwise operators cannot tell a missing binary from a hung runtime.
    #[tokio::test]
    #[cfg(unix)]
    async fn does_not_mask_a_spawn_failure_as_a_ready_timeout() {
        // Given
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When
        let result = manager
            .start_terminal(
                "spawn-mask-session",
                worktree.path().to_path_buf(),
                "/nonexistent/definitely-not-a-real-binary",
            )
            .await;

        // Then
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("spawning a missing binary must fail");
        assert!(
            !message.contains("did not signal ready"),
            "a spawn failure must not be reported as a ready-signal timeout, was: {message}"
        );
    }
}
