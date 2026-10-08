use crate::{cli_session_manager::CliSessionManager, connection_service::StackParentHost};

use std::path::PathBuf;

use tddy_daemon_kernel::config::DaemonConfig;

use super::launch_ports::LaunchSessions;
use super::session_acting_identity::SessionAccountAccess;

use std::sync::Arc;

/// Per-session [`ChildSpawnHandler`] for a PR-stack orchestrator: materializes a planned-PR node
/// into a child claude-cli session (with the orchestrator as `stack_parent`), reusing the same
/// [`spawn_claude_cli_session_inner`] the `StartSession` RPC uses. Bound only to a `pr-stack`
/// orchestrator's toolcall listener, so it can only spawn children for that orchestrator's stack.
pub(crate) struct StackChildSpawnHandler {
    /// Resolves each child's base off the orchestrator's stack. A collaborator rather than
    /// something built here because the orchestrator this handler spawns children of is a session
    /// of *this* daemon, so the resolution never leaves the host — but it takes the one path every
    /// spawn takes, rather than a second one that would drift from it.
    pub(crate) stack_parent_host: Arc<dyn StackParentHost>,

    /// The launch handle whose attachment path materializes the child's documents. A shallow clone
    /// (every shared field is behind an `Arc`), exactly as [`DaemonSeedCloneClaimant`] holds one:
    /// the documents go through [`LaunchSessions::prepare_session_attachments`], the same
    /// materializer `StartSession` uses, so a child cannot differ by how it was started.
    pub(crate) service: LaunchSessions,
    pub(crate) config: DaemonConfig,
    pub(crate) tddy_data_dir: PathBuf,
    pub(crate) claude_cli_manager: Arc<CliSessionManager>,
    pub(crate) os_user: String,
    pub(crate) project_id: String,
    pub(crate) sessions_base: PathBuf,
    pub(crate) orchestrator_session_id: String,
    pub(crate) orchestrator_session_dir: PathBuf,
    /// What the orchestrator's own start read its owner's vault with. A child is a session of the
    /// **same owner** on the **same project** (`os_user`, `project_id` above), so it resolves the
    /// account the project assigns over that owner's vault, as the orchestrator did; the child's
    /// own start never gets a token of its own to read one with. The token stays in daemon memory
    /// and is never handed to the child.
    pub(crate) account_access: SessionAccountAccess,
}
