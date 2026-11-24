//! Following an agent's own transcript file, for an agent whose spec reads activity from it
//! (`coding_agent.activity.transcript`): its lines are mapped by the spec's declared rules and
//! delivered to the daemon's activity handling, through the same [`HookReportDelivery`] a target's
//! hooks reach it by.
//!
//! The file is on the machine the agent runs on, so it is followed through that machine's shell —
//! the session's remote shell on a target (SSH or Coder), `LocalShell` on this host — by the one
//! [`crate::log_follower::FileFollower`] the hook log is followed with: the same byte offsets, the
//! same pty rules, the same resume from the last delivered byte. Its path is rendered from the spec
//! with that machine's values: on a target its home is the target shell's `$HOME` and its worktree
//! the target's, never this host's.
//!
//! The follower is the session's, not its terminal's: an agent run in tmux on a target outlives
//! the client attached to it, and its follower keeps reading across a detach. Its offset is
//! persisted in the session's directory, so the follower a resume or a restarted daemon starts reads
//! on from the last line delivered.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_coding_agent_specs::{TranscriptPathVars, TranscriptSource};
use tddy_remote_shell::{ExecRequest, RemoteShell};

use crate::log_follower::{FileFollower, FollowOffset, FollowedFile, LineSink};
use crate::remote_hook_delivery::HookReportDelivery;

/// Where a session's follower state is kept in its directory.
pub const FOLLOWERS_DIR: &str = "followers";

/// The persisted offset of the session's transcript follower.
#[must_use]
pub fn transcript_offset(session_dir: &Path) -> FollowOffset {
    FollowOffset::persisted_at(session_dir.join(FOLLOWERS_DIR).join("transcript.offset"))
}

/// The persisted offset of the session's hook-log follower.
#[must_use]
pub fn hook_log_offset(session_dir: &Path) -> FollowOffset {
    FollowOffset::persisted_at(session_dir.join(FOLLOWERS_DIR).join("hooks.offset"))
}

/// The lease that keeps a host session's transcript `tail` alive: removed when the follower stops,
/// which ends the `tail` within a second (see [`crate::log_follower::tail_command`]).
#[must_use]
pub fn host_follow_lease(session_dir: &Path) -> PathBuf {
    session_dir.join(FOLLOWERS_DIR).join("transcript.alive")
}

/// Why a session's transcript is not followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptFollowError {
    /// The session recorded no usage context, so its conversation and worktree are unknown.
    NoContext,
    /// The agent's home is unknown.
    NoHome(String),
    /// The spec's path does not render for the session.
    Path(String),
}

impl std::fmt::Display for TranscriptFollowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoContext => write!(
                f,
                "the session recorded no usage context, so its conversation and worktree are unknown"
            ),
            Self::NoHome(why) => write!(f, "the session's agent home is unknown: {why}"),
            Self::Path(why) => write!(f, "the transcript's path cannot be rendered: {why}"),
        }
    }
}

impl std::error::Error for TranscriptFollowError {}

/// The home of the user `shell` runs as on its machine: `$HOME` there.
pub async fn home_on(shell: &dyn RemoteShell) -> Result<String, TranscriptFollowError> {
    let output = shell
        .exec(ExecRequest::new("printf '%s' \"$HOME\""))
        .await
        .map_err(|error| TranscriptFollowError::NoHome(error.to_string()))?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status != 0 || !home.starts_with('/') {
        return Err(TranscriptFollowError::NoHome(format!(
            "`printf $HOME` exited {} with {home:?}",
            output.status
        )));
    }
    Ok(home)
}

/// The path of the session's transcript on the machine its agent runs on: rendered from the
/// session's recorded usage context (its conversation and worktree), with `agent_home` — the
/// target's home on a target, the recorded host home on this host.
pub fn transcript_path(
    transcript: &TranscriptSource,
    session_dir: &Path,
    agent_home: Option<String>,
) -> Result<String, TranscriptFollowError> {
    let context =
        tddy_usage::read_usage_context(session_dir).ok_or(TranscriptFollowError::NoContext)?;
    let agent_home = agent_home
        .or_else(|| {
            context
                .agent_home
                .as_ref()
                .map(|home| home.to_string_lossy().into_owned())
        })
        .ok_or_else(|| TranscriptFollowError::NoHome("none was recorded".to_string()))?;
    let vars = TranscriptPathVars {
        agent_home,
        conversation_id: context.conversation_id.unwrap_or_default(),
        worktree: context
            .worktree
            .map(|worktree| worktree.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    transcript
        .render_path(&vars)
        .map_err(|error| TranscriptFollowError::Path(error.to_string()))
}

/// Who a session's transcript lines are delivered as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptOwner {
    pub session_id: String,
    pub os_user: String,
}

/// Start following `path` on `shell`'s machine, mapping each line by `transcript` and delivering
/// it to `delivery`, from the byte `offset` holds; the remote `tail` lives while `alive_while`
/// exists there.
pub fn follow_transcript(
    shell: Arc<dyn RemoteShell>,
    file: (&str, &str),
    transcript: TranscriptSource,
    owner: TranscriptOwner,
    delivery: Arc<dyn HookReportDelivery>,
    offset: FollowOffset,
) -> FileFollower {
    let (path, alive_while) = file;
    FileFollower::start(
        shell,
        FollowedFile {
            path: path.to_string(),
            alive_while: alive_while.to_string(),
            waits_for_file: true,
            label: "agent transcript",
        },
        offset,
        Arc::new(TranscriptLines {
            transcript,
            owner,
            delivery,
            path: path.to_string(),
        }),
    )
}

/// A followed transcript's lines, mapped and delivered.
struct TranscriptLines {
    transcript: TranscriptSource,
    owner: TranscriptOwner,
    delivery: Arc<dyn HookReportDelivery>,
    path: String,
}

#[async_trait]
impl LineSink for TranscriptLines {
    /// A line that is not JSON, or that no rule maps, says nothing: it is skipped — logged without
    /// its content, which is the conversation's.
    async fn line(&self, line: &[u8], end_offset: u64) {
        let value: serde_json::Value = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(error) => {
                log::warn!(
                    "agent transcript {}: the line ending at byte {end_offset} is not JSON: {error}",
                    self.path
                );
                return;
            }
        };
        let entries = self.transcript.map_line(&value);
        if entries.is_empty() {
            return;
        }
        let delivered = self
            .delivery
            .record_transcript_line(tddy_session_activity::TranscriptLine {
                session_id: self.owner.session_id.clone(),
                os_user: self.owner.os_user.clone(),
                entries,
                end_offset,
            })
            .await;
        if let Err(error) = delivered {
            log::warn!(
                "agent transcript {}: the line ending at byte {end_offset} of session {} was refused: {error}",
                self.path,
                self.owner.session_id
            );
        }
    }
}
