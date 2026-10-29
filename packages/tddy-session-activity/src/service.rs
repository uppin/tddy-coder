//! The `activity.ActivityService` implementation, and the entry the daemon's wiring layer
//! registers.
//!
//! All eight methods answer about **one host's** sessions: six read a session directory the serving
//! host resolves from the caller's own token (or, for the two hook reports, from the OS user whose
//! per-session `hook_token` they present), one answers from the notification bus this host
//! publishes on, and one from the delta ring the session room on this host measured. None of them
//! trusts a path or an OS user the request supplied, which is why the ports below are resolvers
//! rather than values.
//!
//! # What is deliberately not here
//!
//! `daemon_instance_id` routing is **not** implemented in this crate, for the reason node 6's
//! `tddy-session-files` gives: forwarding a call to the peer that holds the transcript needs the
//! common-room slot, the eligible-daemon roster and the LiveKit forwarding clients, all of which
//! are the daemon's transport layer. The daemon wraps this implementation in its own routing layer
//! (`tddy-daemon`'s `PeerRoutedActivity`), which is where forwarding lives — and where the two
//! streaming methods' "forwarding is not supported yet" refusal stays, because it is a statement
//! about the transport's idle deadline rather than about activity.
//!
//! A session's **display label** is not resolved here either: it is read from the daemon's
//! `session_list_enrichment`, which serves `ListSessions` — family C, which this node keeps in the
//! daemon. [`SessionLabels`] is the seam.

mod conversation_replay;
mod hook_reports;
mod operator_notifications;
pub(crate) mod ports;
mod question_answers;
mod rpc;

pub use question_answers::AnswerWaitPorts;

use std::path::PathBuf;

use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES;
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::AgentActivityDeltaChunk;
use tddy_worktree_service::stream::MpscResultStream;

pub use crate::service::ports::{
    ActivityPorts, DeltaLookup, DeltaScope, MeasuredDelta, NoSessionDeltaStores,
    SessionDeltaStores, SessionLabels,
};

/// The `activity.ActivityService` implementation.
pub struct ActivityServiceImpl {
    ports: ActivityPorts,
    /// What `AwaitQuestionAnswer` holds a hook with (`#home-inbox` 6/8).
    answers: AnswerWaitPorts,
}

impl ActivityServiceImpl {
    /// The service over `ports`, holding no hook for an answer ([`AnswerWaitPorts::default`]) until
    /// [`Self::with_question_answers`].
    #[must_use]
    pub fn new(ports: ActivityPorts) -> Self {
        Self {
            ports,
            answers: AnswerWaitPorts::default(),
        }
    }

    /// Hold `AwaitQuestionAnswer` callers with `answers` (builder).
    #[must_use]
    pub fn with_question_answers(mut self, answers: AnswerWaitPorts) -> Self {
        self.answers = answers;
        self
    }

    /// The base directory a token's owner keeps its sessions under.
    fn sessions_base_for_token(&self, session_token: &str) -> Result<PathBuf, Status> {
        let os_user = (self.ports.os_users)(session_token)?;
        self.sessions_base_for_os_user(&os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))
    }

    /// The base directory `os_user` keeps its sessions under, or `None` when the user is unknown
    /// to this host.
    fn sessions_base_for_os_user(&self, os_user: &str) -> Option<PathBuf> {
        tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.ports.tddy_data_dir),
        )
    }

    /// The session directory a token-bearing request addresses, with its id validated as one path
    /// segment first — a session id is untrusted client input that becomes a path component.
    fn session_dir(&self, session_token: &str, session_id: &str) -> Result<PathBuf, Status> {
        let sessions_base = self.sessions_base_for_token(session_token)?;
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(unified_session_dir_path(&sessions_base, session_id))
    }

    /// The sessions base and session directory one of the two **hook** reports addresses.
    ///
    /// A hook carries no web session token — it is a per-worktree `claude` or `cursor` hook naming
    /// the OS user it runs as and presenting that session's own `hook_token`. So the path is
    /// resolved from `os_user` and the credential is checked against the `.session.yaml` this
    /// resolves to, in [`Self::authenticated_hook`] below: the directory has to be found before the
    /// token it holds can be compared.
    fn hook_session_dir(
        &self,
        os_user: &str,
        session_id: &str,
    ) -> Result<(PathBuf, PathBuf), Status> {
        validate_session_id_segment(session_id)
            .map_err(|_| Status::invalid_argument("invalid session_id"))?;
        let sessions_base = self
            .sessions_base_for_os_user(os_user)
            .ok_or_else(|| Status::not_found("unknown os_user or sessions_base not found"))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        Ok((sessions_base, session_dir))
    }

    /// The session a hook names, or `NOT_FOUND` when this host has no such directory.
    ///
    /// The `hook_token` is **not** checked here, and the two callers check it at different points
    /// for a reason the refusals carry: `ReportSessionStatus` refuses a session type that raises no
    /// hooks at all before it looks at the credential, so a `cursor` hook wired to a `workspace`
    /// session is told what is actually wrong rather than that its token is bad.
    fn hook_session(
        &self,
        session_dir: &std::path::Path,
    ) -> Result<tddy_core::SessionMetadata, Status> {
        tddy_core::read_session_metadata(session_dir)
            .map_err(|_| Status::not_found("session not found"))
    }

    /// Raise one notification for `session_id`, owned by `os_user`, when there is one to raise.
    ///
    /// A no-op when this host has no bus. The label is resolved through the port rather than
    /// re-derived, so this notification and the drawer row it belongs to name the same session
    /// identically.
    async fn publish_for_session(
        &self,
        sessions_base: &std::path::Path,
        session_id: &str,
        build: impl FnOnce(&str) -> Option<crate::session_notifications::SessionNotification>,
    ) {
        let Some(bus) = self.ports.notifications.as_ref() else {
            return;
        };
        let label = self
            .ports
            .session_labels
            .label_for(sessions_base, session_id);
        if let Some(notification) = build(&label) {
            bus.publish(notification).await;
        }
    }
}

/// Split one [`MeasuredDelta`]'s patch into ordered [`HOST_DOCUMENT_FRAME_BYTES`] frames.
///
/// Every frame carries the whole description — `seq`, `prev_seq`, `base_commit`,
/// `total_byte_size` and `scoped_paths` — for the reason the wire contract gives: a reader knows
/// what it is receiving from the first frame, and a client can check the server scoped the way it
/// asked rather than trusting that it did.
///
/// A call that changed nothing is **one** frame with an empty patch and `total_byte_size` 0 — AC9.
/// An empty answer must not look like a failed one.
#[must_use]
pub fn activity_delta_frames(delta: &MeasuredDelta) -> Vec<AgentActivityDeltaChunk> {
    let total_byte_size = delta.patch.len() as u64;
    let describe = |patch: Vec<u8>| AgentActivityDeltaChunk {
        patch,
        seq: delta.seq,
        prev_seq: delta.prev_seq,
        base_commit: delta.base_commit.clone(),
        total_byte_size,
        scoped_paths: delta.scoped_paths.clone(),
    };
    let mut frames: Vec<AgentActivityDeltaChunk> = delta
        .patch
        .chunks(HOST_DOCUMENT_FRAME_BYTES)
        .map(|chunk| describe(chunk.to_vec()))
        .collect();
    if frames.is_empty() {
        frames.push(describe(Vec::new()));
    }
    frames
}

/// Send every frame into a fresh channel and hand back the stream reading it.
///
/// The frames are already in memory — whatever produced them applied its own cap or refusal first,
/// so nothing here can be a partial answer. A dropped receiver stops the loop rather than filling a
/// channel nobody reads.
fn streamed<T>(frames: Vec<T>) -> Response<MpscResultStream<T>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<T, Status>>();
    for frame in frames {
        if tx.send(Ok(frame)).is_err() {
            break;
        }
    }
    Response::new(MpscResultStream::from(rx))
}

/// Adapt a plain-item channel to the `Result`-carrying stream the generated trait returns.
///
/// The three live relays produce bare frames because every refusal on those paths is answered as
/// the call's own error before the stream exists — there is no mid-stream `Status` for them to
/// carry. Wrapping here rather than making the relays produce `Result`s keeps that true of the
/// relays themselves.
fn relayed<T: Send + 'static>(
    mut items: tokio::sync::mpsc::UnboundedReceiver<T>,
) -> Response<MpscResultStream<T>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<T, Status>>();
    tokio::spawn(async move {
        while let Some(item) = items.recv().await {
            if tx.send(Ok(item)).is_err() {
                break;
            }
        }
    });
    Response::new(MpscResultStream::from(rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ports::DeltaLookup;
    use crate::service::ports::DeltaScope;
    use crate::service::ports::NoSessionDeltaStores;

    #[test]
    fn with_no_room_hosted_here_every_session_has_no_delta_rather_than_a_failure() {
        let looked_up = NoSessionDeltaStores
            .delta_for_call("1780828020298-abc", "call-1", DeltaScope::Call)
            .expect("no room is an answer, not a failure");

        assert_eq!(looked_up, DeltaLookup::NoRoomHere);
    }
}
