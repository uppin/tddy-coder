use super::HostServiceImpl;

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{
    CoderWorkspace as ListedWorkspace, ListCoderWorkspacesRequest, ListCoderWorkspacesResponse,
    ProbeOutcome,
};

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
        let req = request.into_inner();

        if let Some(answered) = self
            .rpc_served_by_peer("ListCoderWorkspaces", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let subject = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let credentials = match self
            .coder_credentials
            .credentials_for(&subject, &req.account_id)
        {
            Ok(credentials) => credentials,
            Err(reason) => {
                return Ok(Response::new(ListCoderWorkspacesResponse {
                    outcome: ProbeOutcome::Failed as i32,
                    failure_reason: reason.to_string(),
                    workspaces: Vec::new(),
                }));
            }
        };

        match self.coder_workspaces.workspaces(&credentials).await {
            Ok(workspaces) => Ok(Response::new(ListCoderWorkspacesResponse {
                outcome: ProbeOutcome::Ok as i32,
                failure_reason: String::new(),
                workspaces: workspaces
                    .iter()
                    .map(|workspace| ListedWorkspace {
                        owner: workspace.owner.clone(),
                        name: workspace.name.clone(),
                        status: workspace.status.clone(),
                        running: workspace.is_running(),
                    })
                    .collect(),
            })),
            Err(reason) => Ok(Response::new(ListCoderWorkspacesResponse {
                outcome: ProbeOutcome::Failed as i32,
                failure_reason: reason.to_string(),
                workspaces: Vec::new(),
            })),
        }
    }
}
