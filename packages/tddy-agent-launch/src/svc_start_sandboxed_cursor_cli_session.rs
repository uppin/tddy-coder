use std::collections::BTreeMap;
use std::path::Path;

use tddy_session_agents::roster_replacement::roster_replacement_pairs;

use crate::stack_parent;
use tddy_session_agents::agent_roster;
use tddy_session_agents::seed_codebase;
use tddy_session_split::service_util;
use tddy_worktree_service::branch_intent::BranchIntentPolicy;

use tddy_worktree_service::branch_intent::BranchIntentRequest;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::PathBuf;

use crate::claude_jail::{closure_defs, TDDY_TOOLS_ID};
use crate::cursor_cli_spawn::resolve_cursor_agent;
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{harness_template_params, HarnessLaunch};
use crate::jail_launch::{jail_home, mcp_server_env, tool_ipc_socket, JailSpec, JailedSession};
use crate::launch_ports::LaunchSessions;
use crate::spec_launch::{HostAgent, HostCodingAgents};
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};
use tddy_coding_agent_specs::slots::{
    self, cursor::render_cursor_hooks_json, McpServerCommand, SlotInputs,
};
use tddy_core::HookCommandParams;
use tddy_daemon_kernel::config::DaemonConfig;
use uuid::Uuid;

/// A sandboxed `cursor-cli` launch from the `cursor` spec, as its start has settled it.
struct CursorJail<'a> {
    jail: &'a JailSession<'a>,
    dirs: &'a JailDirs,
    model: &'a str,
    /// The chat the session owns, which `--resume` names.
    chat_id: &'a str,
    initial_prompt: &'a str,
    /// The managed workflow's prompt, written whole as the agent's rules file.
    rules: Option<String>,
    /// What the daemon adds for the session: see [`JailedSession::env`].
    env: BTreeMap<String, String>,
    session_env: Vec<(String, String)>,
    managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    managed_codebase: bool,
}

// TODO(crap-svc-start-sandboxed-cursor-cli-session): converted onto `LaunchSessions` by a header-only
// edit (no logic touched), which that code-issue record's "tests first" rule forbids; the
// characterisation tests it asks for are deferred. Linux CI's sandboxed suites are the evidence.
impl LaunchSessions {
    /// Handle `StartSession` for sandboxed `cursor-cli` sessions (darwin Seatbelt / Linux cgroups).
    ///
    /// The agent is launched from the `cursor` spec through `launch_jailed` (`#agent-tools 7/12`). A
    /// host without coding agents refuses with `FailedPrecondition`, as the Claude path does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_sandboxed_cursor_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        // Authorizes the `workspace` starts this session's seeded agents need on their own hosts
        // (see `provision_agent_clone`); the peer sees the same token the client presented here.
        session_token: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        stack_parent: Option<&str>,
        // The daemon whose sessions tree holds `stack_parent` (empty = this one).
        stack_parent_daemon_instance_id: &str,
        // The planned node of that orchestrator's stack this spawn materializes, as the surface
        // that started it named it. Preferred over the branch when the base is resolved (D34).
        stack_node_id: &str,
        initial_prompt: &str,
        managed_codebase: bool,
        specialized_agents: &[String],
        clone_placed_agents: &[String],
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and point the
        // in-jail `SemanticSearch` tool at the per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base only), push the new branch to origin at session start.
        create_remote_branch: bool,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(Status::invalid_argument(
                "project_id is required for cursor-cli sessions",
            ));
        }
        // As on the sandboxed claude-cli path: the roster is resolved before anything is created,
        // and the defs it holds locally are what the jail env can carry.
        let mut started_agents = self
            .agent_roster
            .seeded_roster_records_placed(specialized_agents, clone_placed_agents)
            .await?;
        let specialized_defs = self
            .agent_roster
            .resolve_specialized_agent_defs(specialized_agents)
            .await?;

        // Readiness gate: wake every specialized agent's endpoint and wait until each answers
        // before spawning the jail, so a cold/unreachable model fails session start here rather
        // than stalling the main agent's first subagent call. No fallback — the jail is never
        // spawned if warm-up fails. Resume gates separately, in `relaunch_sandboxed_runner`.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let (_, project) =
            service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)?;
        let repo_root = service_util::project_repo_root(&project)?;

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        let intent = service_util::write_initial_changeset(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent,
                new_branch_name,
                selected_integration_base_ref,
                selected_branch_to_work_on,
            },
            BranchIntentPolicy::cursor_cli(),
            project.main_branch_ref.as_deref(),
            &session_dir,
            stack_parent,
            managed_recipe.as_deref(),
        )?;

        let chain_base_ref = self
            .resolve_chain_base_ref_status(&stack_parent::StackBaseLookup {
                session_token,
                stack_parent,
                stack_parent_daemon_instance_id,
                project_id,
                sessions_base: &sessions_base,
                repo_root: &repo_root,
                new_branch_name,
                stack_node_id,
                selected_integration_base_ref,
            })
            .await?;
        let worktree_base_ref =
            tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);
        let timeout = self.config.spawn_worker_request_timeout();
        let worktree_path = service_util::create_session_worktree(
            timeout,
            "start_sandboxed_cursor_cli_session: create worktree",
            &repo_root,
            &session_dir,
            worktree_base_ref,
        )
        .await?;

        service_util::push_new_branch_to_origin_if_requested(
            create_remote_branch,
            intent,
            &session_dir,
            &worktree_path,
            timeout,
        )
        .await?;

        // With coding agents, the jailed agent is the `cursor` spec's: its hooks file is written
        // into the worktree now, before the jail's context is copied from it.
        let agents = self.coding_agents.clone().ok_or_else(|| {
            Status::failed_precondition(
                "this host has no coding agents installed: a sandboxed cursor-cli session is \
                 launched from the `cursor` tool definition",
            )
        })?;
        let agent = resolve_cursor_agent(&agents, &self.config)?;
        let hook_token = Uuid::new_v4().to_string();
        write_jail_hooks(
            &agent,
            &self.config,
            &worktree_path,
            session_id,
            os_user,
            &hook_token,
        )?;

        // The clones this session's seeded agents read, claimed now: the worktree they mirror
        // exists, and the peers that build them must be at work before the agent that prompts them
        // is launched. Where an agent runs decides how the session is split across hosts, never
        // whether it can be seeded — the same placements the split start takes, this one takes.
        let seeded_clones = self
            .agent_roster
            .claim_co_located_seed_clones(
                session_id,
                &seed_codebase::SeedCodebase::of_a_starting_session(
                    session_dir.clone(),
                    worktree_path.clone(),
                    project_id,
                    // The jail is what puts `mcp__tddy-tools__*` in front of the main agent's file
                    // tools, which is what makes a withdrawal enforceable
                    // (`session_enforces_a_withdrawal`).
                    true,
                ),
                session_token,
                &mut started_agents,
            )
            .await?;

        let sandbox_root = session_dir.join("sandbox");
        let egress_dir = session_dir.join("egress");
        std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
            .map_err(|e| Status::internal(format!("mkdir sandbox scratch: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
            .map_err(|e| Status::internal(format!("mkdir sandbox tmp: {e}")))?;
        std::fs::create_dir_all(sandbox_root.join("context"))
            .map_err(|e| Status::internal(format!("mkdir sandbox context: {e}")))?;
        std::fs::create_dir_all(&egress_dir)
            .map_err(|e| Status::internal(format!("mkdir sandbox egress: {e}")))?;

        let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
        let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
        let scratch_dir = sandbox_root.join(".work");
        let scratch_tmp = scratch_dir.join("tmp");
        let context_dir = sandbox_root.join("context");

        let replacement_pairs = roster_replacement_pairs(&started_agents);
        let replacement_refs: Vec<Vec<&str>> = replacement_pairs
            .iter()
            .map(|(_, tools)| tools.iter().map(String::as_str).collect())
            .collect();
        let replacements: Vec<tddy_sandbox::SubagentReplacement<'_>> = replacement_pairs
            .iter()
            .zip(replacement_refs.iter())
            .map(|((name, _), refs)| tddy_sandbox::SubagentReplacement {
                name,
                replaced: refs,
            })
            .collect();
        let ctx = tddy_daemon_sandbox::sandbox_session::prepare_context_dir_with_subagent(
            &worktree_path,
            &replacements,
            tddy_session_files::context_files::context_globs_for_session_type("cursor-cli"),
        )
        .map_err(Status::internal)?;
        tddy_daemon_sandbox::sandbox_session::copy_dir_all(ctx.path(), &context_dir)
            .map_err(Status::internal)?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            tddy_daemon_kernel::config::resolve_cursor_cli_tddy_tools_path(&self.config).as_deref(),
        );

        let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
        // The jailed agent edits and commits through the host-side tool relay, which runs under
        // this env, so the project's account identity goes here whether or not the session is
        // managed. The token does not: the agent's tools ask the host for it over the session's
        // own socket.
        let identity = self
            .host
            .session_identity(os_user, session_id, project_id, session_token);
        let (mut session_env, github_credential) = self.jail_relay_identity(identity, os_user)?;
        // The managed workflow's prompt, as the agent's rules file.
        let mut managed_rules: Option<String> = None;
        if let Some(recipe) = managed_recipe.clone() {
            let launch = self.prepare_managed_workflow(
                session_id,
                recipe,
                &session_dir,
                &worktree_path,
                &context_dir,
                &tddy_tools_path,
                None,
                None,
                github_credential,
            )?;
            managed_rules = std::fs::read_to_string(&launch.prompt_file).ok();
            session_env.extend(launch.env);
            managed = Some(launch.workflow);
        }

        // Semantic index: index the worktree into the session dir before spawning the jail
        // (blocking; a missing embedder or a failed index aborts the start — no unindexed
        // fallback), and inject `TDDY_SEMANTIC_INDEX_DB` into the jail env so the in-jail
        // `SemanticSearch` tool resolves against the per-session index.
        let mut semantic_index_env_pair: Option<(String, String)> = None;
        if semantic_index {
            service_util::index_session_worktree(
                &self.tddy_data_dir,
                &self.task_registry,
                session_id,
                &worktree_path,
                &session_dir,
            )
            .await?;
            semantic_index_env_pair = Some(
                tddy_semantic_index::semantic_index::semantic_index_env(&session_dir),
            );
        }

        let (pid, cursor_chat_id) = {
            // The chat this session owns for its whole lifetime: minted here, persisted in the
            // session's metadata, and named by `--resume` on every later launch.
            let chat_id = crate::chat::mint_cursor_chat_id(
                &agent.tool.binary.to_string_lossy(),
                &worktree_path,
            )
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "failed to create the Cursor chat for session {session_id}: {e}"
                ))
            })?;
            // The hook token is on record before the agent starts: its first hook fires during
            // the launch, and a report is refused until the session names the token.
            tddy_core::write_session_metadata(
                &session_dir,
                &tddy_core::SessionMetadata {
                    hook_token: Some(hook_token.clone()),
                    managed_codebase,
                    ..tddy_session_split::service_util::starting_session_metadata(
                        session_id,
                        project_id,
                        "cursor-cli",
                    )
                },
            )
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
            let mut env = BTreeMap::new();
            if !specialized_defs.is_empty() {
                env.extend(self.specialized_subagent_env(&specialized_defs)?);
            }
            env.extend(self.jail_daemon_identity_env());
            env.extend(self.lsp_tools_env(&worktree_path));
            env.extend(self.restructure_tools_env());
            env.extend(semantic_index_env_pair);
            let jail = JailSession {
                session_id,
                project_id,
                session_token,
                session_dir: &session_dir,
                worktree_path: &worktree_path,
            };
            let dirs = JailDirs {
                sandbox_root: sandbox_root.clone(),
                egress_dir: egress_dir.clone(),
                scratch_dir: scratch_dir.clone(),
                scratch_tmp: scratch_tmp.clone(),
                context_dir: context_dir.clone(),
            };
            let pid = self
                .launch_jailed_cursor(
                    &agents,
                    &agent,
                    CursorJail {
                        jail: &jail,
                        dirs: &dirs,
                        model,
                        chat_id: &chat_id,
                        initial_prompt,
                        rules: managed_rules,
                        env,
                        session_env,
                        managed,
                        managed_codebase,
                    },
                )
                .await?;
            (pid, Some(chat_id))
        };

        let meta = tddy_core::SessionMetadata {
            repo_path: Some(worktree_path.to_string_lossy().to_string()),
            pid: Some(pid),
            model: Some(model.to_string()),
            cursor_chat_id,
            hook_token: Some(hook_token),
            sandbox: Some(true),
            managed_codebase,
            recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
            agents_rev: agent_roster::started_roster_rev(&started_agents),
            agents: started_agents,
            ..tddy_session_split::service_util::starting_session_metadata(
                session_id,
                project_id,
                "cursor-cli",
            )
        };
        tddy_core::write_session_metadata(&session_dir, &meta)
            .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
        // The roster naming them is on disk now, so the clones belong to the session rather than to
        // the start that claimed them.
        seeded_clones.keep();

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed cursor-cli session {session_id} pid={pid} worktree={}",
            worktree_path.display()
        );

        Ok(Response::new(StartSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
            branch_conflict: None,
        }))
    }

    /// Launch the `cursor` spec's agent in a jail for `launch` and return the runner's pid: its argv
    /// from `cursor.yaml`, its `{home}/.cursor/mcp.json` registration of `tddy-tools` in the
    /// persistent cursor home, and its rules file in the context directory.
    // TODO(cli-dispatch-collapse): deleted at green — § Removed of docs/dev/1-WIP/2026-11-04-agent-tools-cli-dispatch-collapse.md.
    async fn launch_jailed_cursor(
        &self,
        agents: &HostCodingAgents,
        agent: &HostAgent,
        launch: CursorJail<'_>,
    ) -> Result<u32, Status> {
        let defs = closure_defs(agents.registry(), agent)?;
        let tddy_tools = agent
            .dep(TDDY_TOOLS_ID)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "the cursor tool does not depend on {TDDY_TOOLS_ID}, which a jailed agent \
                     reaches its tools through"
                ))
            })?
            .binary
            .clone();
        let tool_def = defs
            .iter()
            .find(|def| def.id == agent.tool.id)
            .ok_or_else(|| {
                Status::internal(format!(
                    "the registry holds no definition of {}",
                    agent.tool.id
                ))
            })?;
        let home = jail_home(
            tool_def,
            &self.tddy_data_dir,
            &launch.dirs.scratch_dir.join("home"),
        )?;
        let server = McpServerCommand {
            name: TDDY_TOOLS_ID.to_string(),
            command: tddy_tools,
            args: vec!["--mcp".to_string()],
            env: mcp_server_env(launch.jail.session_id, launch.dirs, &launch.env),
        };
        let slots = slots::render(
            &agent.spec,
            &SlotInputs {
                workdir: launch.dirs.context_dir.clone(),
                scratch_dir: launch.dirs.scratch_tmp.clone(),
                mcp_server: Some(server),
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
                permission_prompt_tool: None,
                preamble: launch.rules,
                append_system_prompt_file: None,
                // Written into the worktree before the context was copied (`write_jail_hooks`).
                hooks: None,
                home: Some(home),
            },
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        let launcher = node_launcher_of(&agent.tool.binary);
        let params = harness_template_params(&HarnessLaunch {
            model: launch.model,
            prompt: Some(launch.initial_prompt),
            chat_id: Some(launch.chat_id),
            node_launcher: launcher.as_ref(),
            ..HarnessLaunch::default()
        });
        self.launch_jailed(
            JailSpec {
                agent,
                defs: &defs,
                host: agents.host(),
            },
            &params,
            &slots,
            JailedSession {
                jail: launch.jail,
                dirs: launch.dirs,
                session_env: launch.session_env,
                env: launch.env,
                managed: launch.managed,
                managed_codebase: launch.managed_codebase,
                route: None,
            },
            "",
        )
        .await
    }
}

/// Write the jailed agent's `.cursor/hooks.json` into `worktree_path`, each event reporting over the
/// session's tool-IPC socket: the one socket a jail can reach. It is written on the host because the
/// jail reads a copy of the worktree taken after this.
fn write_jail_hooks(
    agent: &HostAgent,
    config: &DaemonConfig,
    worktree_path: &Path,
    session_id: &str,
    os_user: &str,
    hook_token: &str,
) -> Result<(), Status> {
    let tddy_tools = agent.dep(TDDY_TOOLS_ID).ok_or_else(|| {
        Status::failed_precondition(format!(
            "the cursor tool does not depend on {TDDY_TOOLS_ID}"
        ))
    })?;
    let tddy_tools_path = tddy_tools.binary.to_string_lossy().into_owned();
    let daemon_socket = tool_ipc_socket(session_id).to_string_lossy().into_owned();
    let daemon_url = crate::chat::cursor_hook_daemon_url(config);
    let params = HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token,
        daemon_socket: Some(&daemon_socket),
        answer_wait_secs: config.claude_cli_answer_wait_secs(),
    };
    let (path, bytes) = render_cursor_hooks_json(worktree_path, &params)
        .map_err(|e| Status::internal(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Status::internal(format!("create {}: {e}", parent.display())))?;
    }
    tddy_core::atomic_file::write_atomic(&path, bytes)
        .map_err(|e| Status::internal(format!("write {}: {e}", path.display())))
}
