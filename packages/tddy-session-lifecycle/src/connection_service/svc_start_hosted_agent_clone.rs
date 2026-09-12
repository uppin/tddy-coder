// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use prost::Message as _;
use tddy_service::proto::exec_tools::ExecuteToolRequest;

use tddy_service::proto::session_agents_svc::OpenAgentConversationResponse;

use tddy_service::proto::session_agents_svc::OpenAgentConversationRequest;

use std::{path::Path, sync::Arc};

use std::path::PathBuf;

use crate::{
    connection_service::agent_roster, livekit_peer_discovery::local_instance_id_for_config,
    project_storage, workspace_session,
};

use crate::user_sessions_path::projects_path_for_user;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Turn a freshly created `workspace` checkout into a live mirror of the facilitating daemon's
    /// session.
    ///
    /// Spawned rather than awaited: the caller is the facilitating daemon's forwarded
    /// `StartSession`, and holding that open while this joins a room and restores a whole worktree
    /// would push it past a deadline that is already generous for a `git clone`. The mirror reports
    /// its own readiness afterwards (`ReportAgentCloneState`), which is the only account of it that
    /// can be trusted — nothing on the facilitating daemon can see this checkout.
    pub(crate) async fn start_hosted_agent_clone(
        &self,
        placement: &tddy_service::proto::session::AgentClonePlacement,
        sessions_base: &Path,
        codebase_session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let session_id = placement.session_id.trim();
        let facilitating = placement.facilitating_daemon_instance_id.trim();
        if session_id.is_empty() || facilitating.is_empty() {
            return Err(Status::invalid_argument(
                "agent_clone must name both the session it mirrors and the daemon facilitating it; \
                 half a placement names a room to join with nobody in it to address",
            ));
        }
        let (_common_room, url, api_key, api_secret) =
            tddy_daemon_livekit::livekit_peer_discovery::livekit_common_room_connect_strings(
                &self.config,
            )
            .map_err(|e| {
                Status::failed_precondition(format!("this daemon cannot hold an agent clone: {e}"))
            })?;
        let worktree_path = workspace_session::resolve_worktree_root_for_session(
            sessions_base,
            codebase_session_id,
        )?;
        // The repository the checkout was cut from, which is where its WIP ref is fetched from.
        let projects_dir = projects_path_for_user(
            self.config
                .os_user_for_github(
                    &(self.user_resolver)(session_token)
                        .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?,
                )
                .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "project '{project_id}' is not registered here, so an agent clone of it has \
                     nothing to fetch the session's WIP ref from"
                ))
            })?;

        // Only a checkout that was cloned *from the facilitating daemon* fetches its WIP ref over
        // `tddy-remote-git-repo` — its `origin` is the facilitator's `{instance_id}:{project_id}` URL,
        // the only place that ref lives. A checkout the owning daemon already had on the shared
        // filesystem fetches the ref from that local repo directly (the facilitating daemon
        // published it there), so it must NOT carry the transport-shim env var: `origin` there is the
        // forge URL, which `tddy-remote-git-repo` would try to reach and fail. (PRD AC37.)
        let facilitator_origin_prefix = format!("{facilitating}:");
        let is_facilitator_clone = project.git_url.starts_with(&facilitator_origin_prefix);

        let spec = crate::session_agent_clone::CloneMirrorSpec {
            session_id: session_id.to_string(),
            facilitating_daemon_instance_id: facilitating.to_string(),
            owning_daemon_instance_id: local_instance_id_for_config(&self.config),
            codebase_session_id: codebase_session_id.to_string(),
            worktree_path,
            project_repo_path: PathBuf::from(&project.main_repo_path),
            project_id: project_id.to_string(),
            session_token: session_token.to_string(),
            livekit_url: url,
            livekit_api_key: api_key,
            livekit_api_secret: api_secret,
            facilitating_daemon_url: if is_facilitator_clone {
                let u = placement.facilitating_daemon_url.trim();
                if u.is_empty() {
                    None
                } else {
                    Some(u.to_string())
                }
            } else {
                None
            },
            first_admission_token: placement.first_admission_token.clone(),
            first_admission_url: placement.first_admission_url.clone(),
            first_admission_room: placement.first_admission_room.clone(),
            common_room_slot: self.common_room_livekit_room.clone(),
        };
        let hosted = Arc::clone(&self.hosted_agent_clones);
        let clone_id = codebase_session_id.to_string();
        tokio::spawn(async move {
            if let Err(status) = crate::session_agent_clone::run_clone_mirror(spec, hosted).await {
                // Loud and final: the facilitating daemon has already been told the clone failed
                // (the mirror reports before it returns), and there is nothing here that could
                // repair a room this daemon cannot reach.
                log::error!(
                    "agent clone {clone_id} stopped mirroring: {}",
                    status.message()
                );
            }
        });
        Ok(())
    }

    /// Refuse a prompt to an agent whose checkout is not ready to serve reads, naming the state.
    ///
    /// Queuing it would make a 90-second `git clone` look like a hung agent, and serving it would
    /// read an empty checkout and report "not found" for a file that is simply not there yet
    /// (PRD AC33).
    pub(crate) fn refuse_unready_clone(
        &self,
        session_id: &str,
        record: &tddy_core::SessionAgentRecord,
    ) -> Result<(), Status> {
        use tddy_service::proto::session_agents_svc::AgentCloneState;
        let clone = self
            .session_agent_clones
            .get(session_id, &record.daemon_instance_id);
        let (state, error) = match clone {
            Some(clone) => (clone.state, clone.error),
            None => (AgentCloneState::Unspecified, String::new()),
        };
        match state {
            AgentCloneState::Ready | AgentCloneState::Local => Ok(()),
            AgentCloneState::Provisioning => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted yet: its clone on daemon '{}' is still \
                 provisioning",
                record.agent_id, record.daemon_instance_id
            ))),
            AgentCloneState::Error => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted: its clone on daemon '{}' is in the error state \
                 ({error})",
                record.agent_id, record.daemon_instance_id
            ))),
            AgentCloneState::Unspecified => Err(Status::failed_precondition(format!(
                "agent '{}' cannot be prompted: this daemon has no clone on daemon '{}' for \
                 session '{session_id}' — the state is unknown, which is not the same as ready",
                record.agent_id, record.daemon_instance_id
            ))),
        }
    }

    /// Refuse to address an owning daemon that is no longer in the common room.
    ///
    /// Named rather than left to a forward deadline: an agent on a departed daemon must fail *its
    /// own* prompts with an error naming that daemon, while the rest of the roster keeps working
    /// (PRD AC35). Waiting out `PEER_FORWARD_TIMEOUT` reaches the same answer thirty seconds later
    /// and tells the operator only that something timed out.
    pub(crate) async fn refuse_departed_daemon(
        &self,
        daemon_instance_id: &str,
    ) -> Result<(), Status> {
        if self
            .eligible_instance_ids()
            .iter()
            .any(|candidate| candidate == daemon_instance_id)
        {
            return Ok(());
        }
        Err(Status::unavailable(format!(
            "daemon '{daemon_instance_id}' has left the common room, so the agents it owns on this \
             session cannot be reached; the rest of the roster is unaffected"
        )))
    }

    /// Ask the owning daemon to open the conversation on its side, under the id this daemon minted.
    ///
    /// The id travels rather than being minted there, so a forward that times out still leaves this
    /// daemon able to name — and therefore cancel — whatever the peer opened.
    pub(crate) async fn forward_open_agent_conversation(
        &self,
        req: &OpenAgentConversationRequest,
        owner: &str,
        conversation_id: &str,
    ) -> Result<(), Status> {
        let slot = self.common_room_slot("OpenAgentConversation")?;
        let forwarded = OpenAgentConversationRequest {
            conversation_id: conversation_id.to_string(),
            daemon_instance_id: owner.to_string(),
            ..req.clone()
        };
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            owner,
            tddy_session_agents::SERVICE_NAME,
            "OpenAgentConversation",
            forwarded.encode_to_vec(),
        )
        .await?;
        let opened = OpenAgentConversationResponse::decode(answered.as_slice())
            .map_err(|e| Status::internal(format!("decode OpenAgentConversationResponse: {e}")))?;
        if opened.conversation_id != conversation_id {
            return Err(Status::internal(format!(
                "daemon '{owner}' opened conversation {:?} instead of the requested \
                 {conversation_id:?}, so a prompt to it could not be routed and a cancel could not \
                 name it",
                opened.conversation_id
            )));
        }
        Ok(())
    }

    /// A turn loop for an agent this daemon resolves and serves from the session's own worktree.
    pub(crate) async fn open_local_agent_session(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &tddy_core::SessionAgentRecord,
        session_token: &str,
    ) -> Result<Box<dyn tddy_discovery::subagent::SubagentSession>, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // Through the spawn resolver rather than the listing one: a registry assistant's def comes
        // back carrying its provider's credential there, and a session opened without one comes up
        // "successfully" and 401s on every model call.
        let def = self
            .agent_def_for_spawn(&record.name, &github_user)
            .await?
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{}' resolves to no def on this daemon any more",
                    record.agent_id
                ))
            })?;
        Ok(Box::new(
            tddy_discovery::subagent::SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                self.local_agent_codebase_access(
                    session_id,
                    session_dir,
                    &record.agent_id,
                    session_token,
                ),
                def.system_prompt.clone(),
                def.tools.clone(),
            ),
        ))
    }

    /// A turn loop for an agent **this** daemon owns, reading the clone it holds for another
    /// daemon's session.
    pub(crate) async fn open_owned_agent_session(
        &self,
        agent_id: &str,
        clone: &Arc<crate::session_agent_clone::HostedClone>,
    ) -> Result<Box<dyn tddy_discovery::subagent::SubagentSession>, Status> {
        let id = tddy_core::AgentId::parse(agent_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let local_instance_id = local_instance_id_for_config(&self.config);
        if id.daemon_instance_id != local_instance_id {
            return Err(Status::invalid_argument(format!(
                "agent '{agent_id}' is owned by daemon '{}', not by this one ('{local_instance_id}')",
                id.daemon_instance_id
            )));
        }
        let def = self
            .resolvable_agent_defs()
            .await?
            .into_iter()
            .find(|d| d.name == id.name)
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "agent '{agent_id}' resolves to no def on daemon '{local_instance_id}'"
                ))
            })?;
        Ok(Box::new(
            tddy_discovery::subagent::SpecializedSubagentSession::new(
                def.base_url.clone(),
                def.model.clone(),
                def.api_key.clone(),
                def.max_turns,
                self.owned_agent_codebase_access(clone),
                def.system_prompt.clone(),
                def.tools.clone(),
            ),
        ))
    }

    /// How an agent this daemon owns reaches files: reads from its clone, mutations proxied.
    ///
    /// Managed rather than [`CodebaseAccess::Local`] even though the checkout is on this host: the
    /// tool engine is what confines a path to the worktree, and a loop given direct filesystem
    /// access would be one YAML field away from writing anywhere this daemon can.
    pub(crate) fn owned_agent_codebase_access(
        &self,
        clone: &Arc<crate::session_agent_clone::HostedClone>,
    ) -> tddy_discovery::subagent::CodebaseAccess {
        let service = self.clone();
        let clone = Arc::clone(clone);
        tddy_discovery::subagent::CodebaseAccess::managed(move |tool_name, args| {
            let service = service.clone();
            let clone = Arc::clone(&clone);
            Box::pin(async move {
                let request = ExecuteToolRequest {
                    session_token: String::new(),
                    session_id: clone.session_id.clone(),
                    daemon_instance_id: String::new(),
                    tool_name,
                    args_json: args.to_string(),
                };
                agent_roster::dispatch_envelope(
                    service.run_hosted_clone_tool(&request, &clone).await,
                )
            })
        })
    }

    /// How an agent this daemon serves locally reaches files: the session's own worktree, through
    /// the same tool engine every other exec-tool caller goes through.
    pub(crate) fn local_agent_codebase_access(
        &self,
        session_id: &str,
        session_dir: &Path,
        agent_id: &str,
        session_token: &str,
    ) -> tddy_discovery::subagent::CodebaseAccess {
        let service = self.clone();
        let session_token = session_token.to_string();
        let session_id = session_id.to_string();
        let session_dir = session_dir.to_path_buf();
        let agent_id = agent_id.to_string();
        tddy_discovery::subagent::CodebaseAccess::managed(move |tool_name, args| {
            let service = service.clone();
            let session_token = session_token.clone();
            let session_id = session_id.clone();
            let session_dir = session_dir.clone();
            let agent_id = agent_id.clone();
            Box::pin(async move {
                // This dispatch is the only place this daemon sees the agent's own loop enter a
                // tool call, so it is the only place EXECUTING_TOOL can be told from RUNNING.
                service.note_agent_activity(
                    &session_id,
                    &session_dir,
                    &agent_id,
                    crate::session_agent_status::ManagedAgentState::ExecutingTool,
                    crate::session_agent_status::tool_call_summary(&tool_name, &args),
                );
                let request = ExecuteToolRequest {
                    session_token,
                    session_id: session_id.clone(),
                    daemon_instance_id: String::new(),
                    tool_name,
                    args_json: args.to_string(),
                };
                let answer = match service.resolve_exec_tool_worktree(&request) {
                    Ok((sessions_base, worktree_root)) => agent_roster::dispatch_envelope(
                        service
                            .run_exec_tool_locally(&request, &sessions_base, &worktree_root)
                            .await,
                    ),
                    Err(status) => {
                        serde_json::json!({ "is_error": true, "error": status.message() })
                            .to_string()
                    }
                };
                // Back to RUNNING, not IDLE: the tool returned, but the turn that called it has
                // not, and an idle badge on a turn still in flight is the one that misleads.
                service.note_agent_activity(
                    &session_id,
                    &session_dir,
                    &agent_id,
                    crate::session_agent_status::ManagedAgentState::Prompting,
                    format!("{} finished", request.tool_name),
                );
                answer
            })
        })
    }

    /// Record what a roster agent is doing, and push the roster that says so.
    ///
    /// One rule, in `tddy-session-agents`: the nine family-B handlers set the same badges, and two
    /// copies would let a status set on this path — the local agent's own tool dispatch, the only
    /// place this daemon sees a loop *enter* a tool call — disagree with one set on theirs.
    pub(crate) fn note_agent_activity(
        &self,
        session_id: &str,
        session_dir: &Path,
        agent_id: &str,
        state: crate::session_agent_status::ManagedAgentState,
        summary: impl AsRef<str>,
    ) {
        tddy_session_agents::note_agent_activity(
            &self.session_agent_rosters,
            self.hosted_clone_for(session_id).is_some(),
            session_id,
            session_dir,
            agent_id,
            state,
            summary,
        );
    }
}
