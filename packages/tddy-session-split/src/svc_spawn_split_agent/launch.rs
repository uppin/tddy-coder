use crate::split_ports::SplitSessions;

use crate::svc_spawn_split_agent::split_agent_pty::spawn_split_agent_pty;
use crate::svc_spawn_split_agent::split_agent_pty::SplitAgentPty;

use crate::svc_spawn_split_agent::SplitAgentProcess;
use std::path::Path;
use std::sync::Arc;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;
use uuid::Uuid;

impl SplitSessions {
    pub(super) async fn spawn_split_agent_process(
        &self,
        launch: SplitAgentProcess<'_>,
    ) -> Result<
        (
            String,
            Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>,
        ),
        Status,
    > {
        let SplitAgentProcess {
            os_user,
            session_id,
            req,
            initial_prompt,
            tddy_tools_path,
            remote,
            context_dir,
            extra_args,
            session_dir,
        } = launch;
        let hook_token = Uuid::new_v4().to_string();
        // The agent's hook is told to wait this long, so the daemon holds it for as long: recorded
        // beside `.session.yaml`, where `AwaitQuestionAnswer` reads it. A split agent has no tool
        // file of its own to read the wait from, so it is the specs' default.
        let answer_wait_secs = record_split_answer_wait(session_dir)?;
        // The hooks file is the one the split agent's spec's hooks slot renders, written
        // under the context dir the agent runs in.
        self.write_split_hooks(
            os_user,
            session_id,
            tddy_tools_path,
            &context_dir,
            &hook_token,
            answer_wait_secs,
        )
        .await?;
        // The launch argv is rendered from the split agent's spec — the same template a
        // spec-driven launch renders — with this start's model, session, permission mode and
        // prompt bound.
        let (spec, argv) = self.render_split_argv(session_id, req, &initial_prompt)?;
        let argv = split_argv_with_flags(initial_prompt, extra_args, argv);
        let pointer =
            crate::split_session::split_session_pointer(&remote, &context_dir, session_dir);
        // The agent's `tddy-tools` finds its session through the pointer in the context dir (and
        // through its own registration, `extra_args`): the agent process is given no `TDDY_*`
        // variable. The pointer is removed when the agent exits.
        let handle = self
            .place_pointer_and_spawn(
                os_user,
                session_id,
                req.model.trim(),
                context_dir,
                pointer,
                (spec, argv),
            )
            .await?;
        Ok((hook_token, handle))
    }

    async fn write_split_hooks(
        &self,
        os_user: &str,
        session_id: &str,
        tddy_tools_path: std::path::PathBuf,
        context_dir: &std::path::Path,
        hook_token: &str,
        answer_wait_secs: u64,
    ) -> Result<(), Status> {
        let agent_tool_socket = self.agent_tool_socket_of(os_user).await?;
        let hook_params = tddy_core::HookCommandParams {
            tddy_tools_path: &tddy_tools_path.to_string_lossy(),
            daemon_url: &tddy_daemon_kernel::daemon_hook_urls::advertise_daemon_url(&self.config),
            session_id,
            os_user,
            hook_token,
            daemon_socket: Some(&agent_tool_socket),
            answer_wait_secs,
        };
        self.render_and_write_split_hooks(session_id, context_dir, hook_params)?;
        Ok(())
    }

    async fn place_pointer_and_spawn(
        &self,
        os_user: &str,
        session_id: &str,
        model: &str,
        context_dir: std::path::PathBuf,
        pointer: tddy_core::session_pointer::SessionPointer,
        spec_and_argv: (tddy_coding_agent_specs::CodingAgentSpec, Vec<String>),
    ) -> Result<Arc<tddy_cli_sessions::cli_session_manager::PtyHandle>, Status> {
        let (spec, argv) = spec_and_argv;
        let session_type = format!("{}-cli", spec.id);
        let on_exit = self.session_pointers.place(&pointer, &context_dir, None)?;
        let pointer_dir = context_dir.clone();
        let handle = spawn_split_agent_pty(
            &self.cli_manager,
            &spec,
            self.coding_agents.agent_env_base(),
            SplitAgentPty {
                session_id,
                session_type: &session_type,
                context_dir,
                model,
                argv,
                os_user: Some(os_user),
                on_exit,
            },
        )
        .await
        .map_err(|e| {
            // No agent exists whose exit would remove the pointer just placed.
            self.session_pointers.remove(&pointer_dir);
            Status::internal(format!("failed to spawn {session_type}: {e}"))
        })?;
        Ok(handle)
    }

    fn render_and_write_split_hooks(
        &self,
        session_id: &str,
        context_dir: &std::path::Path,
        hook_params: tddy_core::HookCommandParams<'_>,
    ) -> Result<(), Status> {
        match tddy_coding_agent_specs::slots::render(
            &crate::agent_argv::split_agent_spec(&self.coding_agents)?,
            &tddy_coding_agent_specs::slots::SlotInputs {
                workdir: context_dir.to_owned(),
                scratch_dir: context_dir.to_owned(),
                mcp_server: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
                permission_prompt_tool: None,
                preamble: None,
                append_system_prompt_file: None,
                hooks: Some(hook_params),
                home: None,
            },
        ) {
            Ok(rendered) => {
                for (path, bytes) in &rendered.files {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(path, bytes) {
                        log::warn!(
                            "session {session_id}: failed to write {} — hooks will not fire: {e}",
                            path.display()
                        );
                    }
                }
            }
            Err(e) => {
                log::warn!("session {session_id}: failed to render its hooks file: {e}");
            }
        }
        Ok(())
    }

    fn render_split_argv(
        &self,
        session_id: &str,
        req: &StartSessionRequest,
        initial_prompt: &str,
    ) -> Result<(tddy_coding_agent_specs::CodingAgentSpec, Vec<String>), Status> {
        let spec = crate::agent_argv::split_agent_spec(&self.coding_agents)?;
        let argv = spec
            .render_run_argv(
                std::path::Path::new(&(self.agent_binary)(&spec.id)?),
                &[
                    ("session_id", session_id),
                    ("chat_id", session_id),
                    ("model", req.model.trim()),
                    ("permission_mode", req.permission_mode.trim()),
                    ("prompt", initial_prompt.trim()),
                ],
                if req.dangerously_skip_permissions {
                    &["dangerously_skip_permissions"][..]
                } else {
                    &[][..]
                },
            )
            .map_err(|e| Status::internal(format!("rendering the split agent's argv: {e}")))?;
        Ok((spec, argv))
    }
}

fn record_split_answer_wait(session_dir: &Path) -> Result<u64, Status> {
    let answer_wait_secs = tddy_coding_agent_specs::DEFAULT_ANSWER_WAIT_SECS;
    tddy_session_activity::answer_wait::record_answer_wait(
        session_dir,
        std::time::Duration::from_secs(answer_wait_secs),
    )
    .map_err(|e| Status::internal(format!("failed to record the answer wait: {e}")))?;
    Ok(answer_wait_secs)
}

fn split_argv_with_flags(
    initial_prompt: String,
    extra_args: Vec<String>,
    argv: Vec<String>,
) -> Vec<String> {
    let argv = tddy_cli_sessions::cli_session_manager::CliSessionManager::with_launch_flags(
        argv,
        Some(initial_prompt.trim()).filter(|p| !p.is_empty()),
        None,
        &extra_args,
    );
    argv
}
