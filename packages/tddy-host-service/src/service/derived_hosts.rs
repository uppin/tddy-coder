use std::sync::Arc;

use super::HostServiceImpl;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::host::{ListDerivedHostsRequest, ListDerivedHostsResponse, ProbeOutcome};

use crate::derived_hosts::{DerivedHostSource, SshConfigDerivedHosts};

impl HostServiceImpl {
    /// Body of [`HostService::list_derived_hosts`]; the trait method delegates here.
    ///
    /// Routed first, like every listing addressed by `daemon_instance_id`. Then the session is
    /// resolved to its OS user, and each [`DerivedHostSource`] of this daemon is asked for that
    /// user's hosts — [`SshConfigDerivedHosts`] over this service's host user files, with the
    /// routing id this daemon answers on as the parent id, then any source registered
    /// with `with_derived_host_source`. A source that fails does not hide the sources that
    /// listed: the answer is `FAILED` with the first failure's reason and still carries the
    /// hosts that arrived.
    ///
    /// [`HostService::list_derived_hosts`]: tddy_service::proto::host::HostService::list_derived_hosts
    pub(crate) async fn list_derived_hosts_impl(
        &self,
        request: Request<ListDerivedHostsRequest>,
    ) -> Result<Response<ListDerivedHostsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(answered) = self
            .rpc_served_by_peer("ListDerivedHosts", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        let parent_instance_id = local_instance_id_for_config(&self.config);
        let mut sources: Vec<Arc<dyn DerivedHostSource>> =
            vec![Arc::new(SshConfigDerivedHosts::new(
                Arc::clone(&self.host_user_files),
                parent_instance_id.clone(),
            ))];
        sources.extend(self.derived_host_sources.iter().map(Arc::clone));
        let coder_cli = self
            .cached_tooling(&parent_instance_id, &os_user, "")
            .await
            .map_err(Status::internal)?
            .coder_cli;
        sources.push(Arc::new(self.coder_derived_hosts(&github_user, coder_cli)));

        let mut hosts = Vec::new();
        let mut failure_reason: Option<String> = None;
        for source in &sources {
            match source.list(&os_user).await {
                Ok(listed) => hosts.extend(listed),
                Err(reason) => {
                    if failure_reason.is_none() {
                        failure_reason = Some(reason);
                    }
                }
            }
        }

        Ok(Response::new(match failure_reason {
            Some(reason) => ListDerivedHostsResponse {
                outcome: ProbeOutcome::Failed as i32,
                failure_reason: reason,
                hosts,
            },
            None => ListDerivedHostsResponse {
                outcome: ProbeOutcome::Ok as i32,
                failure_reason: String::new(),
                hosts,
            },
        }))
    }
}
