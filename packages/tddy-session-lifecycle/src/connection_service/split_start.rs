use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_service::proto::session::SplitAgentPlacement;

use tddy_rpc::Status;

use crate::workspace_session;

/// Whether a peer's `DeleteSession` failure says the session is not there, as opposed to saying
/// nothing usable about it.
///
/// `session_deletion::delete_session_directory` answers `failed_precondition` for a session id it
/// holds no directory for — it reads as wrong-daemon routing there — and `not_found` is the same
/// answer from any layer that phrases it that way. Both mean the worktree is provably gone with the
/// session that owned it. Every other code, and every transport failure, leaves that unknown, which
/// is a different thing and must not be treated as success.
/// What a split start failed with, as far as the teardown that unwinds it is concerned.
///
/// The distinction is not cosmetic: it decides whether the codebase daemon answering "I have no
/// such session" proves the session was never created, or only that it did not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitStartFailure {
    /// A verdict was reached within the deadline — the peer refused, answered something
    /// unusable, or the agent spawn on this host failed. Whatever the peer holds now is final.
    PeerAnswered,
    /// The forwarded start ran out of time. The peer is still free to finish the work it was
    /// doing, so nothing about its current state is final.
    ForwardDeadline,
}

impl SplitStartFailure {
    /// Classify the error a forwarded start came back with. Only [`Code::DeadlineExceeded`] leaves
    /// the peer still working — every other status means it answered.
    pub(crate) fn from_forward_error(status: &Status) -> Self {
        if status.code == tddy_rpc::Code::DeadlineExceeded {
            Self::ForwardDeadline
        } else {
            Self::PeerAnswered
        }
    }
}

pub(crate) fn peer_has_no_such_session(status: &Status) -> bool {
    matches!(
        status.code,
        tddy_rpc::Code::FailedPrecondition | tddy_rpc::Code::NotFound
    )
}

/// Validate `StartSessionRequest.split_agent`: the agent session, on another daemon, that a
/// `workspace` session is being created to hold the worktree for.
///
/// Only `workspace` sessions accept one, for the same reason `requested_session_id` is restricted
/// that way — it records a fact about a checkout, and no other session type has one to record. A
/// caller sending it with any other type is refused rather than having it dropped: the field is what
/// makes a withdrawal enforceable on that session, so silently ignoring it would accept a pairing
/// that never got written and refuse the operator's agents later, on a session that looks paired
/// from the caller's side.
///
/// An agent clone is refused the same way. Both fields describe what a `workspace` checkout is
/// *for*, and a checkout cannot be both a clone's mirror and a split session's working tree.
///
/// Both halves of the placement are required. A daemon named with no session on it names a host but
/// nothing that works in the checkout — see [`tddy_core::paired_agent`], which reads back
/// what this writes and applies the same rule.
pub(crate) fn resolve_split_agent_placement(
    split_agent: Option<&SplitAgentPlacement>,
    session_type: &str,
    is_agent_clone: bool,
) -> Result<Option<workspace_session::PairedAgentSession>, Status> {
    let Some(placement) = split_agent else {
        return Ok(None);
    };
    let session_type = session_type.trim();
    if session_type != "workspace" {
        return Err(Status::invalid_argument(format!(
            "split_agent is only supported for session_type \"workspace\", not {session_type:?}"
        )));
    }
    if is_agent_clone {
        return Err(Status::invalid_argument(
            "split_agent and agent_clone describe what a workspace checkout is for, and a checkout \
             cannot be both an agent clone's mirror and a split session's working tree",
        ));
    }
    let agent_session_id = placement.session_id.trim();
    let agent_daemon_instance_id = placement.agent_daemon_instance_id.trim();
    if agent_session_id.is_empty() || agent_daemon_instance_id.is_empty() {
        return Err(Status::invalid_argument(format!(
            "split_agent placement is incomplete: session_id and agent_daemon_instance_id are both \
             required to record which agent works in this workspace's worktree (got \
             session_id={agent_session_id:?}, agent_daemon_instance_id={agent_daemon_instance_id:?})"
        )));
    }
    Ok(Some(workspace_session::PairedAgentSession {
        daemon_instance_id: agent_daemon_instance_id.to_string(),
        session_id: agent_session_id.to_string(),
    }))
}

/// Validate `StartSessionRequest.requested_session_id`: the id a caller asks the session to be
/// created under instead of one this daemon generates.
///
/// Only `workspace` sessions accept one, and only because of atomicity: the daemon placing a split
/// session's worktree here has to know the id *before* it forwards the start, so that a forward
/// which errors or times out can still name the session to tear down (see
/// `docs/ft/daemon/remote-managed-worktree.md` § Failure is atomic). Every other session type
/// refuses it rather than ignoring it — a caller that believed it had pinned the id would go on to
/// address a session that does not exist.
///
/// The id becomes a directory name under the sessions base, so it is validated exactly as the id
/// `DeleteSession` is handed.
pub fn resolve_caller_chosen_session_id(
    requested_session_id: &str,
    session_type: &str,
) -> Result<Option<String>, Status> {
    let requested = requested_session_id.trim();
    if requested.is_empty() {
        return Ok(None);
    }
    let session_type = session_type.trim();
    if session_type != "workspace" {
        return Err(Status::invalid_argument(format!(
            "requested_session_id is only supported for session_type \"workspace\", not {session_type:?}"
        )));
    }
    validate_session_id_segment(requested).map_err(|e| {
        Status::invalid_argument(format!("invalid requested_session_id: {}", e.message()))
    })?;
    Ok(Some(requested.to_string()))
}
