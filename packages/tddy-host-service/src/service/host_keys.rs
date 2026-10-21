use super::HostServiceImpl;

use tddy_service::proto::host::ListSshConfigHostsResponse;

use tddy_service::proto::host::ListSshConfigHostsRequest;

use tddy_service::proto::host::HostKeyCandidate;

use tddy_service::proto::host::ListHostKeyCandidatesResponse;

use tddy_service::proto::host::ListHostKeyCandidatesRequest;

use std::sync::Arc;

use tddy_service::proto::host::AddHostKeyOutcome;

use crate::host_prompts::answer_before_expiry;

use crate::host_prompts::PromptKind;

use tddy_rpc::Status;

use tddy_service::proto::host::AddHostKeyResponse;

use tddy_rpc::Response;

use tddy_service::proto::host::AddHostKeyRequest;

use tddy_rpc::Request;

impl HostServiceImpl {
    /// Body of [`HostService::add_host_key`]; the trait method delegates here.
    pub(crate) async fn add_host_key_impl(
        &self,
        request: Request<AddHostKeyRequest>,
    ) -> Result<Response<AddHostKeyResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before the prompt is raised. `daemon_instance_id` names the host whose agent the
        // key is loaded into, and answered locally this call loads it into the agent of whichever
        // daemon the browser happened to be talking to — a private key in the wrong machine's
        // agent, which no later request can take back.
        if let Some(answered) = self
            .rpc_served_by_peer("AddHostKey", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // The agent the key lands in is the one belonging to this host's OS user, resolved exactly
        // as `GetHostTooling` resolves the user whose agent it *reads*: a session that may look at
        // an agent's keys is the session that may add one to it.
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        // Stamped with the GitHub user that raised it, which is what makes it *this* operator's
        // question: nobody else is shown it, and nobody else can spend its one answer.
        let prompt = self.host_prompts.issue(
            &github_user,
            PromptKind::SshKeyPassphrase,
            &req.subject,
            crate::host_registry::now_unix_ms(),
        );
        // Claimed immediately after issuing, because issuing is what puts the prompt on the feed:
        // an operator whose browser answers at once must find a handoff already waiting for them.
        let waiting = self.host_prompts.awaited_answer(&prompt.prompt_id);
        let answer = match waiting {
            Some(handoff) => answer_before_expiry(handoff, &prompt).await,
            // The registry forgot the prompt between issuing it and being asked for its handoff,
            // which for the operator is indistinguishable from one that ran out of time.
            None => None,
        };
        let Some(encrypted_answer) = answer else {
            return Ok(Response::new(crate::host_messages::add_key_failed(
                AddHostKeyOutcome::PromptExpired,
                "nobody answered the passphrase prompt before it expired".to_string(),
            )));
        };

        // Everything that follows blocks — an RSA decrypt, a bcrypt-pbkdf unlock and a socket
        // conversation with the agent — so it runs on the blocking pool rather than parking a
        // runtime worker. It also puts the whole life of the plaintext inside one closure, which
        // ends when the closure returns.
        let keypair = Arc::clone(&self.host_keypair);
        let adder = Arc::clone(&self.ssh_agent_key_adder);
        let files = Arc::clone(&self.host_user_files);
        let subject = req.subject.clone();
        let added = tokio::task::spawn_blocking(move || {
            crate::host_messages::unlock_and_add(
                keypair.as_ref(),
                adder.as_ref(),
                files.as_ref(),
                &os_user,
                &subject,
                &encrypted_answer,
            )
        })
        .await
        // The panic's own message is deliberately not repeated: a panic raised inside the unlock is
        // the one string in this flow that could carry key material with it.
        .map_err(|_| Status::internal("adding this key to the agent did not complete"))?;

        // The outcome only — never the reason, and never anything derived from the answer.
        log::debug!(
            "AddHostKey: {} -> {}",
            req.subject,
            AddHostKeyOutcome::try_from(added.outcome)
                .unwrap_or(AddHostKeyOutcome::Unspecified)
                .as_str_name()
        );
        Ok(Response::new(added))
    }

    /// Body of [`HostService::list_host_key_candidates`]; the trait method delegates here.
    pub(crate) async fn list_host_key_candidates_impl(
        &self,
        request: Request<ListHostKeyCandidatesRequest>,
    ) -> Result<Response<ListHostKeyCandidatesResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed first, for the reason the add is: these paths are files on one machine, and a
        // listing answered locally shows the browser this daemon's keys as though they were the
        // addressed host's — after which the path it picks names nothing over there.
        if let Some(answered) = self
            .rpc_served_by_peer("ListHostKeyCandidates", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // The same mapping `add_host_key` resolves the read through, so what an operator is offered
        // and what they may then add are the keys of one OS user — their own.
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        // Off the runtime worker: the listing runs a child process per user-privileged step, the
        // way every other per-user read in this daemon does.
        let files = Arc::clone(&self.host_user_files);
        let candidates = tokio::task::spawn_blocking(move || {
            crate::host_private_key::list_key_candidates(files.as_ref(), &os_user)
        })
        .await
        .map_err(|_| Status::internal("listing the keys on this host did not complete"))?;

        log::debug!("ListHostKeyCandidates: {} offered", candidates.len());
        Ok(Response::new(ListHostKeyCandidatesResponse {
            candidates: candidates
                .into_iter()
                .map(|candidate| HostKeyCandidate {
                    path: candidate.path.display().to_string(),
                    key_type: candidate.key_type,
                    fingerprint: candidate.fingerprint,
                })
                .collect(),
        }))
    }

    /// Body of [`HostService::list_ssh_config_hosts`]; the trait method delegates here.
    pub(crate) async fn list_ssh_config_hosts_impl(
        &self,
        request: Request<ListSshConfigHostsRequest>,
    ) -> Result<Response<ListSshConfigHostsResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        if let Some(answered) = self
            .rpc_served_by_peer("ListSshConfigHosts", &req.daemon_instance_id, &req)
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

        let files = Arc::clone(&self.host_user_files);
        let listed = tokio::task::spawn_blocking(move || {
            crate::ssh_config::list_ssh_config_hosts(files.as_ref(), &os_user)
        })
        .await
        .map_err(|_| Status::internal("listing SSH config hosts on this host did not complete"))?;

        match listed {
            Ok(hosts) => {
                log::debug!(
                    "ListSshConfigHosts: {} explicit aliases",
                    hosts.aliases.len()
                );
                Ok(Response::new(ListSshConfigHostsResponse {
                    outcome: tddy_service::proto::host::ProbeOutcome::Ok as i32,
                    failure_reason: String::new(),
                    hosts: hosts
                        .aliases
                        .into_iter()
                        .map(|alias| tddy_service::proto::host::SshConfigHost { alias })
                        .collect(),
                }))
            }
            Err(crate::ssh_config::SshConfigHostsError::Unreadable(reason)) => {
                Ok(Response::new(ListSshConfigHostsResponse {
                    outcome: tddy_service::proto::host::ProbeOutcome::Failed as i32,
                    failure_reason: reason,
                    hosts: Vec::new(),
                }))
            }
        }
    }
}
