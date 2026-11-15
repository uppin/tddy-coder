use super::HostServiceImpl;

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{ListDerivedHostsRequest, ListDerivedHostsResponse};

impl HostServiceImpl {
    /// Body of [`HostService::list_derived_hosts`]; the trait method delegates here.
    ///
    /// Routed first, like every listing addressed by `daemon_instance_id`. Then the session is
    /// resolved to its OS user, and each [`crate::derived_hosts::DerivedHostSource`] of this
    /// daemon is asked for that user's hosts —
    /// [`crate::derived_hosts::SshConfigDerivedHosts`] over this service's host user files, with
    /// the parent id this daemon is listed under in `ListKnownHosts`. A source's `Err` is the
    /// listing's `FAILED` and its reason.
    ///
    /// [`HostService::list_derived_hosts`]: tddy_service::proto::host::HostService::list_derived_hosts
    pub(crate) async fn list_derived_hosts_impl(
        &self,
        request: Request<ListDerivedHostsRequest>,
    ) -> Result<Response<ListDerivedHostsResponse>, Status> {
        self.record_rpc_activity();
        // TODO(derived-hosts): implement
        let _ = (request.into_inner(), &self.host_user_files);
        Err(Status::unimplemented(
            "listing derived hosts is not implemented on this daemon yet",
        ))
    }
}
