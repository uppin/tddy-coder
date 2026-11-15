use super::HostServiceImpl;

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{ListCoderWorkspacesRequest, ListCoderWorkspacesResponse};

impl HostServiceImpl {
    /// Body of [`HostService::list_coder_workspaces`]; the trait method delegates here.
    ///
    /// Routed first, like every listing addressed by `daemon_instance_id`. Then the session is
    /// resolved to its subject, the account's credentials are read through
    /// [`crate::coder_workspaces::CoderCredentialSource`] — before the network, so a locked vault
    /// fails without a request leaving this daemon — and the deployment is asked through
    /// [`crate::coder_workspaces::CoderWorkspaceSource`].
    ///
    /// [`HostService::list_coder_workspaces`]: tddy_service::proto::host::HostService::list_coder_workspaces
    pub(crate) async fn list_coder_workspaces_impl(
        &self,
        request: Request<ListCoderWorkspacesRequest>,
    ) -> Result<Response<ListCoderWorkspacesResponse>, Status> {
        self.record_rpc_activity();
        // TODO(coder-shell): implement
        let _ = (
            request.into_inner(),
            &self.coder_credentials,
            &self.coder_workspaces,
        );
        Err(Status::unimplemented(
            "listing Coder workspaces is not implemented on this daemon yet",
        ))
    }
}
