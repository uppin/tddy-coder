use crate::worktree_source::sandbox_claude_passthrough_args;

use crate::worktree_source::WorktreeSource;

use crate::worktree_source::session_worktree_source;

use tddy_session_agents::seed_codebase;
use tddy_session_split::service_util;
use tddy_worktree_service::branch_intent::BranchIntentPolicy;

use tddy_worktree_service::branch_intent::BranchIntentRequest;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::sync::Arc;

use std::path::{Path, PathBuf};

use crate::claude_jail::ClaudeJail;
use crate::launch_ports::LaunchSessions;

use tddy_core::provider_route::HarnessRoute;

/// The session a sandboxed start is building a jail for: who it is and where it lives on the host.
pub struct JailSession<'a> {
    pub session_id: &'a str,
    pub project_id: &'a str,
    /// The caller's session token — what the session's tools' account lookups read the vault with.
    pub session_token: &'a str,
    pub session_dir: &'a Path,
    pub worktree_path: &'a Path,
}

/// The branch a sandboxed start works on, and the PR-stack node it materializes: what cutting its
/// worktree and linking that branch back to the orchestrator both read.
pub struct JailBranch<'a> {
    pub session_id: &'a str,
    pub session_token: &'a str,
    pub sessions_base: &'a Path,
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
    pub stack_parent: Option<&'a str>,
    pub stack_parent_daemon_instance_id: &'a str,
    pub stack_node_id: &'a str,
    pub create_remote_branch: bool,
}

/// The per-session jail directories, created and resolved to their canonical paths.
pub struct JailDirs {
    pub sandbox_root: PathBuf,
    pub egress_dir: PathBuf,
    pub scratch_dir: PathBuf,
    pub scratch_tmp: PathBuf,
    pub context_dir: PathBuf,
}

/// A managed-workflow session's controller, its orchestration prompt file and its host-side env.
pub type ManagedJailEnv = (
    Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

/// What the env the daemon adds to a jailed session is built from: its subagents and the semantic
/// index.
struct JailSessionEnv<'a> {
    session_id: &'a str,
    semantic_index: bool,
    specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    session_dir: &'a Path,
    worktree_path: &'a Path,
}

impl LaunchSessions {
    /// Handle `StartSession` for sandboxed `claude-cli` sessions (darwin Seatbelt, local gRPC).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_sandboxed_claude_cli_session(
        &self,
        os_user: &str,
        session_id: &str,
        // Authorizes the `workspace` starts this session's seeded agents need on their own hosts
        // (see `provision_agent_clone`); the peer sees the same token the client presented here.
        session_token: &str,
        sessions_base: PathBuf,
        model: &str,
        project_id: &str,
        // Client-supplied local checkout to run against directly (StartSessionRequest.repo_path).
        // When non-empty it wins over `project_id`: the session's worktree IS this path (no git
        // worktree is created, no registered project is required, and it is never removed on
        // session end). Empty → resolve the worktree from the registered `project_id` as before.
        repo_path: &str,
        branch_worktree_intent: &str,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
        selected_branch_to_work_on: &str,
        // Passed to `claude` as a trailing positional (first user turn) after any pass-through args.
        initial_prompt: &str,
        // Extra args forwarded verbatim to the in-jail `claude` (StartSessionRequest.claude_args).
        claude_args: &[String],
        permission_mode: &str,
        dangerously_skip_permissions: bool,
        stack_parent: Option<&str>,
        // The daemon whose sessions tree holds `stack_parent` (empty = this one).
        stack_parent_daemon_instance_id: &str,
        // The planned node of that parent's stack this child materializes (empty = derive it from
        // the branch, locally — see `record_spawn_on_stack_node`).
        stack_node_id: &str,
        // Specialized subagents (see docs/ft/coder/specialized-subagents.md). This sandboxed path
        // already never mounts the repo (`mounts: vec![]` below, unconditionally) —
        // `managed_codebase` does not toggle mount behavior; it is recorded in the session's
        // metadata so `ListSessions` reports the session as managed. Names resolve against
        // `<tddyhome>/agents` (+ builtins) and are wired into the jail env; all configuration
        // (model, base_url, max_turns, replaces) comes exclusively from the resolved def.
        managed_codebase: bool,
        specialized_agents: &[String],
        clone_placed_agents: &[String],
        // When `Some`, launch workflow-aware: inject the recipe's orchestration prompt and route the
        // agent's host-side `tddy-tools transition` to a per-session `WorkflowController`.
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
        // When true, index the worktree before launch (blocking; aborts on failure) and expose the
        // in-jail `SemanticSearch` tool backed by that per-session index.
        semantic_index: bool,
        // When true (new_branch_from_base + registered project), push the new branch to origin.
        create_remote_branch: bool,
        // The provider route the agent runs on, resolved by the caller before the session's
        // directory was created. `None` runs the harness on its own vendor.
        route: Option<HarnessRoute>,
        // The session's standing role (`project-home` for a project's main; empty for none),
        // recorded in its metadata before the jail is launched.
        session_role: &str,
    ) -> Result<Response<StartSessionResponse>, Status> {
        if model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        let project_id = project_id.trim();
        let repo_path = repo_path.trim();
        // The roster this session starts with, resolved before anything is created for it: it is
        // what the withdrawal is computed from and what `.session.yaml` persists, and a reference
        // naming no agent must fail the start rather than leave the main agent holding tools it was
        // told it had given away.
        let (mut started_agents, specialized_defs) = self
            .warm_up_jail_agents(specialized_agents, clone_placed_agents)
            .await?;

        let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

        // A client-supplied `repo_path` runs against that checkout directly (no registered
        // project), so a stored default branch only applies when resolving from `project_id`.
        let project_default_branch_ref =
            self.project_default_branch_ref(os_user, project_id, repo_path);

        let intent = service_util::write_initial_changeset(
            session_id,
            &BranchIntentRequest {
                branch_worktree_intent,
                new_branch_name,
                selected_integration_base_ref,
                selected_branch_to_work_on,
            },
            BranchIntentPolicy::claude_cli(),
            project_default_branch_ref.as_deref(),
            &session_dir,
            stack_parent,
            managed_recipe.as_deref(),
        )?;

        // Resolve the session's worktree. A client-supplied `repo_path` is used directly (arbitrary
        // local checkout, edited via the host-side tool relay as the caller's mapped OS user); it is
        // never wrapped in a daemon-managed git worktree and never removed on session end. Otherwise
        // fall back to the registered project and create a git worktree as before.
        let branch = JailBranch {
            session_id,
            session_token,
            sessions_base: &sessions_base,
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            stack_parent,
            stack_parent_daemon_instance_id,
            stack_node_id,
            create_remote_branch,
        };
        let worktree_path = match session_worktree_source(repo_path, project_id) {
            WorktreeSource::Project(pid) => {
                if pid.is_empty() {
                    return Err(Status::invalid_argument(
                        "project_id is required for claude-cli sessions",
                    ));
                }
                let (projects_dir, project) =
                    service_util::find_registered_project(&self.tddy_data_dir, os_user, &pid)?;
                let repo_root = service_util::project_repo_root(&project)?;
                let wt = self
                    .create_jail_project_worktree(&branch, &session_dir, intent, &pid, &repo_root)
                    .await?;
                // The branch this spawn works on now exists — record it on the orchestrator's planned
                // node (see `link_stack_node_to_spawned_branch`), keyed on the effective branch so a
                // resumed branch re-links its node. Only this arm resolves a project worktree; a
                // client-supplied `repo_path` materializes no planned node.
                self.link_jail_branch_to_stack_node(
                    &branch,
                    &session_dir,
                    &pid,
                    &projects_dir,
                    &repo_root,
                )
                .await?;
                wt
            }
            WorktreeSource::RepoPath(path) => {
                let canonical = std::fs::canonicalize(&path).map_err(|e| {
                    Status::invalid_argument(format!(
                        "repo_path {} is not accessible: {e}",
                        path.display()
                    ))
                })?;
                if !canonical.is_dir() {
                    return Err(Status::invalid_argument(format!(
                        "repo_path {} is not a directory",
                        canonical.display()
                    )));
                }
                log::info!(
                    target: "tddy_daemon::connection_service",
                    "start_sandboxed_claude_cli_session {session_id}: using client-supplied repo_path {} directly as worktree (not daemon-managed; not removed on session end)",
                    canonical.display()
                );
                canonical
            }
        };

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

        let jail = JailSession {
            session_id,
            project_id,
            session_token,
            session_dir: &session_dir,
            worktree_path: &worktree_path,
        };
        let jail_dirs = jail_session_files::prepare_jail_dirs(&session_dir)?;

        jail_session_files::prepare_jail_context_dir(
            &started_agents,
            &worktree_path,
            &jail_dirs.context_dir,
        )?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Managed workflow: build the per-session controller + toolcall listener, write the recipe's
        // orchestration prompt into the jail-visible context dir, and prepare the per-session env
        // (TDDY_SOCKET + PATH) applied to host-side `tddy-tools transition` (run via the Shell relay).
        let (managed, append_system_prompt_file, session_env) = self.managed_jail_env(
            &jail,
            os_user,
            &sessions_base,
            &managed_recipe,
            &jail_dirs.context_dir,
            &tddy_tools_path,
        )?;

        // The daemon's additions to the jail's environment. Semantic index: index the worktree into
        // the session dir before spawning the jail (blocking; a missing embedder or a failed index
        // aborts the start — no unindexed fallback), and inject `TDDY_SEMANTIC_INDEX_DB`. Its
        // presence both points the in-jail `SemanticSearch` tool at the per-session index and keeps
        // `SemanticSearch` in the agent's tool set (it is otherwise folded into the replaced set).
        let env = self
            .jail_session_env(JailSessionEnv {
                session_id,
                semantic_index,
                specialized_defs,
                session_dir: &session_dir,
                worktree_path: &worktree_path,
            })
            .await?;

        // The `claude` tool definition decides the jail: its home, files, secrets, policy and
        // network. Client-supplied pass-through args and the first prompt go ahead of the slot
        // flags, where the variadic MCP flag cannot swallow them.
        let pid = self
            .launch_jailed_claude(ClaudeJail {
                jail: &jail,
                dirs: &jail_dirs,
                model,
                resume: false,
                permission_mode,
                dangerously_skip_permissions,
                passthrough_args: sandbox_claude_passthrough_args(claude_args, initial_prompt),
                append_system_prompt_file: append_system_prompt_file.as_deref(),
                agents: &started_agents,
                env,
                session_env,
                managed,
                managed_codebase,
                route,
                role: session_role,
            })
            .await?;

        jail_session_files::write_jail_session_metadata(
            &jail,
            model,
            managed_recipe,
            started_agents,
            pid,
            managed_codebase,
            session_role,
        )?;
        // The roster naming them is on disk now, so the clones belong to the session rather than to
        // the start that claimed them.
        seeded_clones.keep();

        log::info!(
            target: "tddy_daemon::connection_service",
            "started sandboxed claude-cli session {session_id} pid={pid} worktree={}",
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

    /// What the daemon adds to the jail's environment for a session: its subagents, who the daemon
    /// is, the LSP and restructure tools, the semantic index and the renderer choice. What the
    /// jail's own environment holds (its `HOME`, `TMPDIR`, the runner's control variables) is the
    /// launcher's and the tool definition's.
    async fn jail_session_env(
        &self,
        launch: JailSessionEnv<'_>,
    ) -> Result<std::collections::BTreeMap<String, String>, Status> {
        let JailSessionEnv {
            session_id,
            semantic_index,
            specialized_defs,
            session_dir,
            worktree_path,
        } = launch;
        let semantic_index_env_pair = self
            .jail_semantic_index_env(session_id, semantic_index, session_dir, worktree_path)
            .await?;
        let mut env = std::collections::BTreeMap::new();
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));
        env.extend(self.restructure_tools_env());
        env.extend(semantic_index_env_pair);
        // The jail's env is built here, not inherited, so the renderer choice has to be carried in.
        env.extend(self.config.claude_cli_renderer_env());
        Ok(env)
    }
}

pub use crate::{jail_env_builders, jail_launch_steps, jail_session_files, jail_worktree};
