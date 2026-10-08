use crate::connection_service::conversation_worktree_op::run_conversation_worktree_op;
use crate::connection_service::daemon_rpc_handler::BoundJailSession;
use crate::connection_service::DaemonSessionHost;
use prost::Message as _;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ConversationWorktreeRequest, ConversationWorktreeResponse};

impl DaemonSessionHost {
    /// Serve a `ConversationWorktree` request relayed out of the jail built for `bound`.
    ///
    /// Unlike the exec-tool RPC this carries no session token to check. The jail is trusted for
    /// exactly the one session it was built for, so a request naming any other session is refused
    /// (`PermissionDenied`) — the runner rewrites every request to its own session, and one that
    /// names another did not come through the runner. The worktree is read from the session
    /// directory the daemon resolved for that jail under the session owner's sessions base, the
    /// same `.session.yaml` the token route reads.
    pub(crate) async fn conversation_worktree_from_jail(
        &self,
        bound: &BoundJailSession,
        payload: &[u8],
    ) -> Result<Vec<u8>, Status> {
        let req = ConversationWorktreeRequest::decode(payload).map_err(|e| {
            Status::invalid_argument(format!("decode ConversationWorktreeRequest: {e}"))
        })?;
        if req.session_id != bound.session_id {
            log::warn!(
                "ConversationWorktree: the jail of session {} named session {:?}; refused",
                bound.session_id,
                req.session_id
            );
            return Err(Status::permission_denied(format!(
                "this jail serves session {}, not {:?}",
                bound.session_id, req.session_id
            )));
        }
        let op = req
            .op
            .ok_or_else(|| Status::invalid_argument("ConversationWorktree carries no operation"))?;
        let worktree_root =
            crate::connection_service::peer_session_answer::resolve_worktree_root_in_session_dir(
                &bound.session_dir,
            )?;
        let result_json =
            run_conversation_worktree_op(&worktree_root, &req.session_id, &req.conversation_id, op)
                .await?;
        Ok(ConversationWorktreeResponse { result_json }.encode_to_vec())
    }
}
