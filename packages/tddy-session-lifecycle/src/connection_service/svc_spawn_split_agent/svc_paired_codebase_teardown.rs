use super::DaemonSessionHost;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_rpc::Status;

use std::path::Path;

use super::super::peer_has_no_such_session;

use tddy_service::proto::session::DeleteSessionRequest;

use super::super::SplitStartFailure;

use livekit::prelude::Room;

use std::sync::Arc;

impl DaemonSessionHost {
    /// Delete the `workspace` session holding a split session's worktree on `codebase_instance_id`.
    ///
    /// Used only to unwind a failed start, where the caller already has an error to return: the
    /// failure that got us here is the more useful one, so a teardown failure is logged with the
    /// orphaned session named rather than replacing it.
    ///
    /// `unwinding` is what the start failed with, which is the only thing that decides how much the
    /// peer's answer proves — see the `peer_has_no_such_session` arm below.
    pub(crate) async fn tear_down_codebase_session(
        &self,
        slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
        unwinding: SplitStartFailure,
    ) {
        let request = DeleteSessionRequest {
            session_token: session_token.to_string(),
            session_id: codebase_session_id.to_string(),
        };
        match tddy_daemon_livekit::livekit_peer_discovery::forward_delete_session_via_livekit(
                slot,
                codebase_instance_id,
                &request,
            )
            .await
            {
                Ok(_) => log::info!(
                    "StartSession: tore down workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start"
                ),
                // A start that failed before the peer created anything is the ordinary case here — the
                // teardown is issued blind, because a forward that never answered leaves this side
                // unable to tell what the peer got as far as building.
                //
                // "Not there" is a statement about *now*, not about the whole start. After a forward
                // deadline the peer may still be cutting the worktree, so it can answer this honestly
                // and create the session moments later — the one case that still orphans a checkout,
                // and the reason that case is a warning an operator can grep for rather than an info
                // line saying nothing was created.
                Err(e) if peer_has_no_such_session(&e) => match unwinding {
                    SplitStartFailure::PeerAnswered => log::info!(
                        "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, after a failed split start"
                    ),
                    SplitStartFailure::ForwardDeadline => log::warn!(
                        "StartSession: daemon {codebase_instance_id} did not have workspace session {codebase_session_id} at teardown time, but the forwarded start had already timed out: if that daemon was still building the worktree it may create the session after this teardown, leaving an orphaned checkout there"
                    ),
                },
                Err(e) => log::error!(
                    "StartSession: could not delete workspace session {codebase_session_id} on daemon {codebase_instance_id} after a failed split start ({e}); its worktree is now orphaned there"
                ),
            }
    }

    /// Delete the `workspace` session paired with a split session, on the daemon that holds its
    /// worktree. A no-op for a co-located session, which records no pairing.
    ///
    /// Unlike the failed-start teardown, a failure here is returned: `DeleteSession` succeeding
    /// while the worktree survives on another host is exactly the silent leak this pairing exists to
    /// prevent, so the message names the session left behind and where.
    pub(crate) async fn delete_paired_codebase_session(
        &self,
        sessions_base: &Path,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let Ok(meta) = read_session_metadata(&session_dir) else {
            return Ok(());
        };
        let Some((codebase_daemon, codebase_session)) = crate::split_session::split_pairing(&meta)
        else {
            return Ok(());
        };

        // A **jailed-codebase** session records this daemon as its own codebase host, so the
        // checkout it names is a `workspace` session right here. Deleted through the same handler
        // an operator's `DeleteSession` reaches, which is what stops its jail and removes its
        // worktree — going out to the common room for a session on this filesystem would fail on
        // a daemon that never joined one, and this placement is defined by not needing one.
        //
        // No recursion beyond this hop: the workspace half records `agent_*`, not `codebase_*`,
        // so `split_pairing` answers `None` for it.
        if codebase_daemon == local_instance_id_for_config(&self.config) {
            let deleted = Box::pin(self.delete_session_at_session_coordinate(
                tddy_rpc::Request::direct(DeleteSessionRequest {
                    session_token: session_token.to_string(),
                    session_id: codebase_session.to_string(),
                }),
            ))
            .await;
            match deleted {
                    Ok(_) => log::info!(
                        "DeleteSession: deleted the jailed checkout session {codebase_session} this session was paired with"
                    ),
                    // The same idempotency the cross-host arm below applies, for the same reason: the
                    // checkout already being gone is the state this call exists to reach, not a
                    // failure to reach it. An operator may have deleted that session directly, or an
                    // earlier attempt may have removed it and then failed on this half. Propagating
                    // here would make the agent session permanently undeletable — every retry would
                    // re-ask for a session that is provably not there.
                    Err(e) if peer_has_no_such_session(&e) => log::info!(
                        "DeleteSession: this daemon no longer has the paired checkout session {codebase_session} ({e}); it was already torn down, so this session's deletion continues"
                    ),
                    Err(e) => return Err(e),
                }
            return Ok(());
        }

        let slot = self.common_room_slot("DeleteSession")?;
        // `common_room_slot` only proves this daemon is *configured* for a common room, not that it
        // is currently joined to one — the discovery loop empties this slot on every disconnect.
        // The distinction is load-bearing here: a forward attempted with no room fails locally with
        // `failed_precondition`, which is the same code the peer returns for "I do not have that
        // session". Without this check the two are indistinguishable, and a momentary disconnect
        // would be read as "already torn down", completing the local delete and stranding the
        // worktree on the codebase host — the exact leak the paired teardown exists to prevent.
        if slot.read().await.is_none() {
            return Err(Status::failed_precondition(format!(
                    "cannot reach the common room to delete the paired workspace session \
                 {codebase_session} on daemon {codebase_daemon}, so its worktree's fate is unknown; \
                 this session was left in place — retry once the daemons can see each other, or \
                 delete that session on {codebase_daemon} directly and retry"
                )));
        }
        match tddy_daemon_livekit::livekit_peer_discovery::forward_delete_session_via_livekit(
                slot,
                codebase_daemon,
                &DeleteSessionRequest {
                    session_token: session_token.to_string(),
                    session_id: codebase_session.to_string(),
                },
            )
            .await
            {
                Ok(_) => log::info!(
                    "DeleteSession: deleted paired workspace session {codebase_session} on daemon {codebase_daemon}"
                ),
                // The peer answering "I do not have that session" is the state this call exists to
                // reach, not a failure to reach it: an operator may have deleted it there directly, or
                // an earlier attempt may have succeeded on the peer and then failed locally. Continuing
                // is idempotency — the worktree is provably gone with the session that owned it. It is
                // deliberately *not* the treatment for any other outcome: an unreachable or failing peer
                // leaves the worktree's fate unknown, and unknown is refused below.
                Err(e) if peer_has_no_such_session(&e) => log::info!(
                    "DeleteSession: daemon {codebase_daemon} no longer has the paired workspace session {codebase_session} ({e}); it was already torn down, so this session's deletion continues"
                ),
                Err(e) => {
                    return Err(Status::internal(format!(
                        "could not delete the workspace session {codebase_session} holding this session's worktree on daemon {codebase_daemon} ({e}); its worktree would be orphaned, so the deletion was refused"
                    )))
                }
            }
        Ok(())
    }
}
