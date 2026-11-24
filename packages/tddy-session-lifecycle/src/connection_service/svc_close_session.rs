//! `PreviewCloseSession` and `CloseSession` — the sessions drawer's × and trash bin: terminate every
//! agent tab of a session, then remove every worktree it owns, as one task
//! (docs/ft/daemon/session-liveness.md § Closing a session).

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    CloseSessionPreview, CloseSessionRequest, PreviewCloseSessionRequest,
};

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// What closing `request.session_id` would lose, per worktree it owns, and the tabs it would end.
    pub(crate) async fn preview_close_session_at_host(
        &self,
        _request: Request<PreviewCloseSessionRequest>,
    ) -> Result<Response<CloseSessionPreview>, Status> {
        // TODO(session-close): read every worktree the session owns and what removing it would lose.
        Err(Status::unimplemented("PreviewCloseSession is not implemented yet"))
    }

    /// Run the close as a `close_session` task, streaming its steps.
    pub(crate) async fn close_session_at_host(
        &self,
        _request: Request<CloseSessionRequest>,
    ) -> Result<Response<crate::CloseSessionEventStream>, Status> {
        // TODO(session-close): check, terminate every tab, re-check, remove every worktree, record.
        Err(Status::unimplemented("CloseSession is not implemented yet"))
    }
}
