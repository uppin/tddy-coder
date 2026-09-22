//! Spawning a Telegram-started session: the chain integration-base merge, the spawn
//! configuration and inputs, and the branch-name helpers the start paths share.

use super::*;

// ---------------------------------------------------------------------------
// Session chaining Phase 2 — chain integration base merge
// ---------------------------------------------------------------------------

/// **Phase 2**: `true` when Telegram spawn applies [`merge_chain_integration_base_with_explicit_operator_overrides`]
/// for chained children before [`TelegramWorkflowSpawn::spawn_blocking`].
pub fn session_chaining_phase2_chain_base_merge_ready() -> bool {
    true
}

/// Merge parent-derived chain integration base with explicit operator overrides (`changeset.yaml` /
/// `worktree_integration_base_ref`).
///
/// **Merge rules (PRD):**
/// 1. If the operator set a non-empty `worktree_integration_base_ref` on the child changeset (remote branch
///    pick), validate it as a chain PR ref and **return it unchanged** — explicit choice wins.
/// 2. Otherwise resolve `origin/<parent-branch>` via [`resolve_chain_integration_base_ref_from_parent_session`]
///    (parent must have branch + `repo_path` aligned with `child_project_repo`).
/// 3. When the child session directory and project repo exist on disk, apply the resolved ref through
///    [`integrate_chain_base_into_session_worktree_bootstrap`] so worktree bootstrap matches [`session_chain_acceptance`].
pub fn merge_chain_integration_base_with_explicit_operator_overrides(
    sessions_root: &Path,
    parent_session_id: &str,
    child_session_dir: &Path,
    child_project_repo: &Path,
    explicit_worktree_integration_base_ref: Option<&str>,
) -> anyhow::Result<String> {
    log::info!(
        target: "tddy_daemon::telegram_session_control",
        "merge_chain_integration_base_with_explicit_operator_overrides: parent={} child_dir={} child_repo={} explicit={:?}",
        parent_session_id,
        child_session_dir.display(),
        child_project_repo.display(),
        explicit_worktree_integration_base_ref
    );

    if let Some(raw) = explicit_worktree_integration_base_ref {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            validate_chain_pr_integration_base_ref(trimmed).map_err(|e| anyhow::anyhow!(e))?;
            log::debug!(
                target: "tddy_daemon::telegram_session_control",
                "merge_chain_integration_base: using explicit operator worktree_integration_base_ref={}",
                trimmed
            );
            return Ok(trimmed.to_string());
        }
    }

    let resolved = resolve_chain_integration_base_ref_from_parent_session(
        sessions_root,
        parent_session_id,
        child_project_repo,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    log::debug!(
        target: "tddy_daemon::telegram_session_control",
        "merge_chain_integration_base: resolved parent-derived ref={}",
        resolved
    );

    if child_session_dir.is_dir() && child_project_repo.exists() {
        integrate_chain_base_into_session_worktree_bootstrap(
            sessions_root,
            parent_session_id,
            child_session_dir,
            child_project_repo,
            &resolved,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        log::info!(
            target: "tddy_daemon::telegram_session_control",
            "merge_chain_integration_base: integrated chain base into worktree bootstrap child_dir={}",
            child_session_dir.display()
        );
    } else {
        log::debug!(
            target: "tddy_daemon::telegram_session_control",
            "merge_chain_integration_base: skip integrate (child_dir or repo missing) child_dir_is_dir={} repo_exists={}",
            child_session_dir.is_dir(),
            child_project_repo.exists()
        );
    }

    Ok(resolved)
}

pub(super) fn projects_dir_for_telegram_workflow_spawn(
    deps: &TelegramWorkflowSpawn,
) -> anyhow::Result<PathBuf> {
    match &deps.projects_dir_override {
        Some(p) => Ok(p.clone()),
        None => projects_path_for_user(&deps.os_user, Some(&deps.tddy_data_dir))
            .ok_or_else(|| anyhow::anyhow!("could not resolve projects path")),
    }
}

pub(super) fn sorted_projects_for_workflow_spawn(
    deps: &TelegramWorkflowSpawn,
) -> anyhow::Result<Vec<ProjectData>> {
    let projects_dir = projects_dir_for_telegram_workflow_spawn(deps)?;
    let mut projects = project_storage::read_projects(&projects_dir)?;
    projects.sort_by(|a, b| a.project_id.cmp(&b.project_id));
    Ok(projects)
}

pub(super) fn read_recipe_from_changeset(session_dir: &Path) -> anyhow::Result<Option<String>> {
    let path = session_dir.join("changeset.yaml");
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let snap: ChangesetRoutingSnapshot = serde_yaml::from_str(&raw)?;
    Ok(snap.recipe)
}

/// Spawn + LiveKit configuration for Telegram-driven workflow start (same invariants as web `StartSession`).
#[derive(Clone)]
pub struct TelegramWorkflowSpawn {
    pub config: Arc<DaemonConfig>,
    pub spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    pub os_user: String,
    /// tddy home data directory (config is the single source of truth).
    pub tddy_data_dir: PathBuf,
    /// When set (e.g. integration tests), read `projects.yaml` from this directory instead of `~/.tddy/projects`.
    pub projects_dir_override: Option<PathBuf>,
    pub telegram_hooks: Option<Arc<TelegramDaemonHooks>>,
    /// Full session id → child gRPC port (for [`crate::presenter_intent_client`]).
    pub child_grpc_by_session: Arc<Mutex<HashMap<String, u16>>>,
    /// Same cache as [`crate::telegram_notifier::TelegramSessionWatcher`] — full strings for select confirmations.
    pub elicitation_select_options: ElicitationSelectOptionsCache,
    /// Multi-select **Choose recommended** metadata (recommended_other keyed by presenter session id).
    pub elicitation_multi_select_meta: ElicitationMultiSelectMetaCache,
    /// Chat id → session id (full) when the user tapped "Other" and we await a free-text follow-up message.
    pub pending_elicitation_other: Arc<Mutex<HashMap<i64, String>>>,
    /// Shared registry of active Claude Code CLI sessions, injected so Telegram-launched sessions
    /// are attachable via the terminal-stream RPCs (same `Arc` as `DaemonSessionHost`).
    pub claude_cli_manager: Arc<crate::cli_session_manager::CliSessionManager>,
}

/// What a Telegram-started workflow session runs, resolved from config and the project registry
/// before any spawn backend is involved.
struct TelegramSpawnInputs {
    livekit: spawner::LiveKitCreds,
    tool_path: String,
    repo_path: PathBuf,
    agent: Option<String>,
    recipe: Option<String>,
    mouse: bool,
}

impl TelegramWorkflowSpawn {
    /// Resolve and validate what to spawn: LiveKit credentials, the project's working copy, the
    /// tool, and the agent/recipe the request asked for.
    fn resolve_spawn_inputs(
        &self,
        project_id: &str,
        agent: Option<&str>,
        recipe: Option<&str>,
    ) -> anyhow::Result<TelegramSpawnInputs> {
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| anyhow::anyhow!("LiveKit not configured"))?;
        let projects_dir = projects_path_for_user(&self.os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| anyhow::anyhow!("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id)?
            .ok_or_else(|| anyhow::anyhow!("project not found"))?;
        let repo_path = PathBuf::from(&project.main_repo_path);
        if !repo_path.exists() {
            anyhow::bail!("project main repo path does not exist");
        }
        if let Some(a) = agent {
            let a = a.trim();
            if !a.is_empty() {
                let allowed = self.config.allowed_agents();
                if !allowed.is_empty() && !allowed.iter().any(|x| x.id == a) {
                    anyhow::bail!(
                        "agent id {:?} is not listed in allowed_agents (configure daemon YAML)",
                        a
                    );
                }
            }
        }
        Ok(TelegramSpawnInputs {
            livekit,
            tool_path: self.config.default_tool_path(),
            repo_path,
            agent: agent
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_string),
            recipe: recipe
                .map(normalize_recipe_name_for_tddy_coder_cli)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            mouse: self.config.spawn_mouse,
        })
    }

    /// Blocking spawn (call from [`tokio::task::spawn_blocking`]).
    pub fn spawn_blocking(
        &self,
        project_id: &str,
        agent: Option<&str>,
        recipe: Option<&str>,
        new_session_id: &str,
    ) -> anyhow::Result<spawner::SpawnResult> {
        let inputs = self.resolve_spawn_inputs(project_id, agent, recipe)?;
        let opts = telegram_spawn_options(&inputs, project_id, new_session_id);
        let coder_log_yaml =
            spawner::coder_log_config_yaml(self.config.coder_config_path.as_deref());
        if let Some(ref client) = self.spawn_client {
            let req = spawn_worker::build_spawn_request(
                &self.os_user,
                &inputs.tool_path,
                &self.tddy_data_dir,
                &inputs.repo_path,
                &inputs.livekit,
                opts,
                self.config.log.as_ref(),
                coder_log_yaml,
                spawner::StartupWatch::from_config(&self.config),
            );
            client.spawn(req)
        } else {
            let (child_log_level, child_log_format) =
                spawner::child_log_yaml_tuning(self.config.log.as_ref());
            spawner::spawn_as_user(
                &self.os_user,
                &inputs.tool_path,
                &self.tddy_data_dir,
                &inputs.repo_path,
                &inputs.livekit,
                opts,
                child_log_level.as_str(),
                child_log_format.as_str(),
                coder_log_yaml.as_deref(),
                spawner::StartupWatch::from_config(&self.config),
            )
        }
    }

    /// Spawn through whichever backend this host uses, bounded by `spawn_worker_request_timeout`.
    ///
    /// With a supervisor configured, the session is started by it — an unreachable supervisor fails
    /// the start rather than spawning the session as the daemon's own user.
    pub async fn spawn_session(
        &self,
        project_id: &str,
        agent: Option<&str>,
        recipe: Option<&str>,
        new_session_id: &str,
    ) -> anyhow::Result<spawner::SpawnResult> {
        let timeout = self.config.spawn_worker_request_timeout();
        match tddy_spawn::supervisor_client::spawn_backend_choice(&self.config) {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                let inputs = self.resolve_spawn_inputs(project_id, agent, recipe)?;
                let req = spawn_worker::build_spawn_request(
                    &self.os_user,
                    &inputs.tool_path,
                    &self.tddy_data_dir,
                    &inputs.repo_path,
                    &inputs.livekit,
                    telegram_spawn_options(&inputs, project_id, new_session_id),
                    self.config.log.as_ref(),
                    spawner::coder_log_config_yaml(self.config.coder_config_path.as_deref()),
                    spawner::StartupWatch::from_config(&self.config),
                );
                match tokio::time::timeout(
                    timeout,
                    tddy_spawn::supervisor_spawn::spawn_session_via_supervisor(&socket_path, &req),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        anyhow::bail!("spawn via tddy-supervisor timed out after {:?}", timeout)
                    }
                }
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                let deps = self.clone();
                let project_id = project_id.to_string();
                let agent = agent.map(str::to_string);
                let recipe = recipe.map(str::to_string);
                let new_session_id = new_session_id.to_string();
                let join = tokio::task::spawn_blocking(move || {
                    deps.spawn_blocking(
                        &project_id,
                        agent.as_deref(),
                        recipe.as_deref(),
                        &new_session_id,
                    )
                });
                match tokio::time::timeout(timeout, join).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(join_e)) => anyhow::bail!("spawn task join: {join_e}"),
                    Err(_elapsed) => anyhow::bail!("spawn timed out after {:?}", timeout),
                }
            }
        }
    }
}

/// The child flags a Telegram-started workflow session gets, identical whichever backend starts it.
fn telegram_spawn_options<'a>(
    inputs: &'a TelegramSpawnInputs,
    project_id: &'a str,
    new_session_id: &'a str,
) -> SpawnOptions<'a> {
    SpawnOptions {
        resume_session_id: None,
        new_session_id: Some(new_session_id),
        project_id: Some(project_id),
        agent: inputs.agent.as_deref(),
        // A Telegram spawn names a config-allowlist agent, which the child resolves for itself.
        agent_def_json: None,
        mouse: inputs.mouse,
        recipe: inputs.recipe.as_deref(),
        stack_parent: None,
        stack_node_id: None,
        // A Telegram spawn names no PR-stack base session: it has no orchestrator to seed.
        stack_seed_base_session: None,
        model: None,
        // Telegram-spawned sessions don't wire the reverse spawn_conversation channel.
        // TODO(stdio-relay): telegram path.
        host_session_socket: None,
    }
}

/// First free `<branch>-<n>` in `repo_root` — read-only (it creates no branch), and run off the
/// reactor because it shells out to `git`.
pub(super) async fn first_free_suffixed_branch_name_off_reactor(
    repo_root: &Path,
    branch: &str,
) -> anyhow::Result<String> {
    let repo_root = repo_root.to_path_buf();
    let branch = branch.to_string();
    tokio::task::spawn_blocking(move || {
        tddy_core::worktree::first_free_suffixed_branch_name(&repo_root, &branch)
    })
    .await
    .map_err(|e| anyhow::anyhow!("suggested branch name join: {e}"))
}

/// Branch name for cursor-cli Telegram sessions: `cursor-cli/<first-8-chars-of-session-id>`.
pub(super) fn cursor_cli_branch_name_from_session_id(session_id: &str) -> String {
    let short_id = &session_id[..8.min(session_id.len())];
    format!("cursor-cli/{short_id}")
}

/// Derive a git branch name for a `/start-claude` session from the changeset `name` field.
///
/// Returns `Some("feature/<slug>")` when `cs.name` is set, `None` otherwise.
/// The slug replaces non-alphanumeric characters with hyphens and collapses consecutive hyphens —
/// matching the logic in `tddy_core::worktree::slugify_for_branch`.
pub(super) fn claude_cli_branch_name_from_changeset(cs: &Changeset) -> Option<String> {
    cs.name.as_deref().map(|name| {
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        format!("feature/{}", slug)
    })
}
