use super::HostServiceImpl;

use tddy_service::proto::host::{
    GetHostToolingResponse, KnownHostEntry, ListEligibleDaemonsRequest, ListEligibleDaemonsResponse,
};

use tddy_service::proto::host::GetHostToolingRequest;

use tddy_service::proto::host::ListKnownHostsResponse;

use tddy_service::proto::host::ListKnownHostsRequest;

use tddy_service::proto::host::EligibleDaemonEntry;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use tddy_rpc::Status;

use tddy_rpc::Response;

use tddy_rpc::Request;

impl HostServiceImpl {
    /// Body of [`HostService::list_eligible_daemons`]; the trait method delegates here.
    pub(crate) async fn list_eligible_daemons_impl(
        &self,
        request: Request<ListEligibleDaemonsRequest>,
    ) -> Result<Response<ListEligibleDaemonsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let daemons: Vec<EligibleDaemonEntry> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|entry| EligibleDaemonEntry {
                instance_id: entry.instance_id.0.clone(),
                label: entry.label,
                is_local: entry.instance_id.0 == local_id,
            })
            .collect();

        Ok(Response::new(ListEligibleDaemonsResponse { daemons }))
    }

    /// Body of [`HostService::list_known_hosts`]; the trait method delegates here.
    pub(crate) async fn list_known_hosts_impl(
        &self,
        request: Request<ListKnownHostsRequest>,
    ) -> Result<Response<ListKnownHostsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        // The durable roster, not the routing one: the registry files a machine under the id that
        // survives its restarts, and intersecting those two id spaces would report the daemon
        // serving this very call as an offline stranger, next to a second row for itself.
        let live_roster = self.eligible_daemon_source.live_known_hosts();
        let local = crate::host_registry::local_host_sighting(&self.config);
        let now_unix_ms = crate::host_registry::now_unix_ms();
        let hosts: Vec<KnownHostEntry> = self
            .host_registry
            .known_hosts(&live_roster, &local, now_unix_ms)
            .into_iter()
            .map(|view| KnownHostEntry {
                instance_id: view.host.instance_id,
                label: view.host.label,
                online: view.online,
                first_seen_unix_ms: view.host.first_seen_unix_ms,
                last_seen_unix_ms: view.host.last_seen_unix_ms,
                repos_base_path: view.host.repos_base_path,
                max_attachment_bytes: view.host.max_attachment_bytes,
                is_local: view.is_local,
            })
            .collect();

        Ok(Response::new(ListKnownHostsResponse { hosts }))
    }

    /// Body of [`HostService::get_host_tooling`]; the trait method delegates here.
    pub(crate) async fn get_host_tooling_impl(
        &self,
        request: Request<GetHostToolingRequest>,
    ) -> Result<Response<GetHostToolingResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Route before probing. Answered locally, a question about another host would come back
        // with this daemon's own git identity under that host's name — a wrong answer that reads
        // exactly like a right one.
        if let Some(answered) = self
            .rpc_served_by_peer("GetHostTooling", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = &self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let local_instance_id = local_instance_id_for_config(&self.config);
        let tooling = self
            .cached_tooling(&local_instance_id, os_user, &req.github_hostname)
            .await
            .map_err(Status::internal)?;

        Ok(Response::new(GetHostToolingResponse {
            daemon_instance_id: local_instance_id,
            git: Some(crate::host_messages::git_identity_message(&tooling.git)),
            github_cli: Some(crate::host_messages::github_cli_message(
                &tooling.github_cli,
            )),
            ssh_agent: Some(crate::host_messages::ssh_agent_message(&tooling.ssh_agent)),
            remote_desktop: tooling
                .remote_desktop
                .iter()
                .map(crate::host_messages::host_remote_desktop_message)
                .collect(),
        }))
    }
}
