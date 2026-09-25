use crate::connection_service::service_util;

use super::DaemonSessionHost;

use super::ToolSpawnPurpose;

use super::ToolSpawnPlan;

use super::super::AttachmentMaterialization;

use tddy_projects::project_storage;
use tddy_spawn::{spawn_worker, spawner};
use uuid::Uuid;

use super::super::recipe_enables_conversation_spawn;

use tddy_daemon_kernel::trim_to_option;

use std::path::Path;

use tddy_rpc::Status;

use super::super::AttachmentProgressSink;

use tddy_service::proto::session::StartSessionRequest;

impl DaemonSessionHost {
    pub(super) async fn spawn_tool_session(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
        os_user: &str,
        agent_def: Option<tddy_discovery::agent_def::SpecializedAgentDef>,
        livekit: spawner::LiveKitCreds,
        project: &project_storage::ProjectData,
    ) -> Result<spawner::SpawnResult, Status> {
        log::debug!("StartSession: entering spawn_blocking session_id=new");
        let os_user = os_user.to_string();
        // The spawn closure below takes ownership; the presenter observer started afterwards needs
        // the same user to resolve the session's label from its sessions directory.
        let observer_os_user = os_user.clone();
        let tool_path = req.tool_path.clone();
        let repo_path = Path::new(&project.main_repo_path).to_path_buf();
        let livekit = livekit.clone();
        let pid_for_spawn = project.project_id.clone();
        let agent_for_spawn = trim_to_option(&req.agent);
        // A spawned `tddy-coder` resolves `--agent` against the builtins and `<tddyhome>/agents`
        // only; this daemon's registry is a source it cannot read. So the def this daemon already
        // resolved travels with the spawn as `--agent-def`, and the child creates its backend from
        // that rather than falling through to a different agent entirely.
        let agent_def_for_spawn: Option<String> = agent_def
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| Status::internal(format!("failed to serialize agent def: {e}")))?;
        let recipe_for_spawn = trim_to_option(&req.recipe);
        let stack_parent_for_spawn = trim_to_option(&req.stack_parent);
        // The planned node the surface that rendered Start-session named. Carried to the child as
        // `--stack-node-id`, which is what puts the association in its participant metadata (D37).
        let stack_node_id_for_spawn = trim_to_option(&req.stack_node_id);
        // Already validated above; the orchestrator's own process is what seeds the stack, because
        // the session that owns a `changeset.yaml` is the process that writes it.
        let stack_seed_base_session_for_spawn = trim_to_option(&req.pr_stack_base_session_id);
        let model_for_spawn = trim_to_option(&req.model);
        // Grill-me tool sessions relay `spawn_conversation` back over a per-session unix socket.
        // Because the coder needs the socket path (and orchestrator id) at spawn time — and the
        // socket path is what crosses the forked `spawn_worker` boundary — bind it and pre-generate
        // the session id BEFORE the spawn, so both the worker and direct paths carry it identically.
        let enable_conversation_spawn = recipe_for_spawn
            .as_deref()
            .map(recipe_enables_conversation_spawn)
            .unwrap_or(false);
        let (mut pre_session_id, host_session_socket): (Option<String>, Option<String>) =
            if enable_conversation_spawn {
                let sid = Uuid::now_v7().to_string();
                let sock = self
                    .spawn_host_session_socket(
                        &sid,
                        &os_user,
                        &pid_for_spawn,
                        model_for_spawn.clone(),
                    )
                    .await;
                (Some(sid), sock)
            } else {
                (None, None)
            };
        let tool_session_id = pre_session_id
            .clone()
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        if enable_conversation_spawn || !req.attachments.is_empty() {
            let sessions_base = crate::user_sessions_path::sessions_base_for_user(
                &os_user,
                Some(&self.tddy_data_dir),
            )
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
            self.prepare_session_attachments(&AttachmentMaterialization {
                session_token: &req.session_token,
                os_user: &os_user,
                sessions_base: &sessions_base,
                session_id: &tool_session_id,
                attachments: &req.attachments,
                progress,
            })
            .await?;
            pre_session_id = Some(tool_session_id);
        }
        let result = self
            .spawn_tddy_coder(ToolSpawnPlan {
                purpose: ToolSpawnPurpose::Start,
                os_user,
                tool_path,
                repo_path,
                livekit,
                resume_session_id: None,
                new_session_id: pre_session_id,
                project_id: Some(pid_for_spawn),
                agent: agent_for_spawn,
                agent_def_json: agent_def_for_spawn,
                recipe: recipe_for_spawn,
                stack_parent: stack_parent_for_spawn,
                stack_node_id: stack_node_id_for_spawn,
                stack_seed_base_session: stack_seed_base_session_for_spawn,
                model: model_for_spawn,
                host_session_socket,
            })
            .await?;
        log::debug!(
            "StartSession: spawn returned, session_id={}",
            result.session_id
        );
        self.maybe_spawn_presenter_observer(
            &observer_os_user,
            &result.session_id,
            result.grpc_port,
        );
        Ok(result)
    }

    /// Spawn a `tddy-coder` child for a starting or resuming tool session, through whichever
    /// backend the config chooses: `tddy-supervisor`, or the forked spawn worker (or, without
    /// one, a direct spawn) on the blocking pool. Both run under the spawn deadline.
    pub(in crate::connection_service) async fn spawn_tddy_coder(
        &self,
        plan: ToolSpawnPlan,
    ) -> Result<spawner::SpawnResult, Status> {
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let tddy_data_dir = self.tddy_data_dir.clone();
        let timeout = self.config.spawn_worker_request_timeout();
        let daemon_log = self.config.log.clone();
        let startup_watch = spawner::StartupWatch::from_config(&self.config);
        let coder_config_path = self.config.coder_config_path.clone();
        let purpose = plan.purpose;
        let result = match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let coder_log_yaml = spawner::coder_log_config_yaml(coder_config_path.as_deref());
                let spawn_req = spawn_worker::build_spawn_request(
                    &plan.os_user,
                    &plan.tool_path,
                    &tddy_data_dir,
                    &plan.repo_path,
                    &plan.livekit,
                    plan.options(spawn_mouse),
                    daemon_log.as_ref(),
                    coder_log_yaml,
                    startup_watch,
                );
                service_util::await_supervised_with_timeout(
                    timeout,
                    purpose.supervisor_label(),
                    tddy_spawn::supervisor_spawn::spawn_session_via_supervisor(
                        &socket_path,
                        &spawn_req,
                    ),
                )
                .await?
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                service_util::spawn_blocking_with_timeout(
                    timeout,
                    purpose.worker_label(),
                    move || {
                        if purpose == ToolSpawnPurpose::Start {
                            log::debug!(
                                "StartSession: spawn_blocking running, using_spawn_worker={}",
                                spawn_client.is_some()
                            );
                        }
                        let coder_log_yaml =
                            spawner::coder_log_config_yaml(coder_config_path.as_deref());
                        if let Some(ref client) = spawn_client {
                            let spawn_req = spawn_worker::build_spawn_request(
                                &plan.os_user,
                                &plan.tool_path,
                                &tddy_data_dir,
                                &plan.repo_path,
                                &plan.livekit,
                                plan.options(spawn_mouse),
                                daemon_log.as_ref(),
                                coder_log_yaml,
                                startup_watch,
                            );
                            client.spawn(spawn_req)
                        } else {
                            let (child_log_level, child_log_format) =
                                spawner::child_log_yaml_tuning(daemon_log.as_ref());
                            spawner::spawn_as_user(
                                &plan.os_user,
                                &plan.tool_path,
                                &tddy_data_dir,
                                &plan.repo_path,
                                &plan.livekit,
                                plan.options(spawn_mouse),
                                child_log_level.as_str(),
                                child_log_format.as_str(),
                                coder_log_yaml.as_deref(),
                                startup_watch,
                            )
                        }
                    },
                )
                .await?
            }
        };
        Ok(result)
    }
}
