//! Spawning a Telegram-started session: the chain integration-base merge, the spawn
//! configuration and inputs, and the branch-name helpers the start paths share.

use super::*;
use tddy_session_lifecycle::connection_service::session_acting_identity::{
    ProjectGithub, SessionAccountAccess,
};

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
    /// Full session id → child gRPC port (for [`tddy_session_lifecycle::presenter_intent_client`]).
    pub child_grpc_by_session: Arc<Mutex<HashMap<String, u16>>>,
    /// Same cache as [`crate::telegram_notifier::TelegramSessionWatcher`] — full strings for select confirmations.
    pub elicitation_select_options: ElicitationSelectOptionsCache,
    /// Multi-select **Choose recommended** metadata (recommended_other keyed by presenter session id).
    pub elicitation_multi_select_meta: ElicitationMultiSelectMetaCache,
    /// Chat id → session id (full) when the user tapped "Other" and we await a free-text follow-up message.
    pub pending_elicitation_other: Arc<Mutex<HashMap<i64, String>>>,
    /// Shared registry of active Claude Code CLI sessions, injected so Telegram-launched sessions
    /// are attachable via the terminal-stream RPCs (same `Arc` as `DaemonSessionHost`).
    pub claude_cli_manager: Arc<tddy_session_lifecycle::cli_session_manager::CliSessionManager>,
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
    /// The commit identity (and GitHub source) of a Local gh project; empty for any other project,
    /// whose vault account a Telegram start cannot open.
    git_environment: Vec<(String, String)>,
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
            git_environment: telegram_git_environment(
                &project,
                &self.os_user,
                &tddy_host_service::GithubCli::on_spawn_child_path(),
            ),
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
        // TODO(tool-route): carry the route through this path
        provider_route_json: None,
        mouse: inputs.mouse,
        recipe: inputs.recipe.as_deref(),
        stack_parent: None,
        stack_node_id: None,
        // A Telegram spawn names no PR-stack base session: it has no orchestrator to seed.
        stack_seed_base_session: None,
        model: None,
        // No host-session socket: the socket answers a session's `github_token` and
        // `spawn_conversation` from a registration the daemon's own session host makes at start
        // (`DaemonSessionHost::register_tool_session_on_host_socket`), and this path starts a
        // session without that host. Without a registration the socket would answer every request
        // with a refusal, so the flag is left off and the agent's request is refused as having no
        // credential handler. Telegram has never wired `spawn_conversation`; that is not a
        // regression. TODO(stdio-relay): give Telegram-started tool sessions the per-user socket,
        // which needs the daemon's session host (and its sockets) handed to this crate.
        host_session_socket: None,
        // A vault account is not resolved here: the vault is opened only by the signed-in owner's
        // session token, and a Telegram start has an OS user and a Telegram-to-GitHub-login link but
        // **no session token** — using the login to reach an already-open vault would let a
        // Telegram chat bypass that gate. Such a session starts under the checkout's own identity,
        // and the chat is told so (`notify_account_identity_unavailable`); the same refusal the
        // daemon's own starts apply, not a token fallback. A Local gh project needs no vault: it
        // carries the host's `gh` identity (`telegram_git_environment`).
        git_environment: &inputs.git_environment,
    }
}

/// The commit identity a Telegram start of `project` carries, as `os_user`.
///
/// A Telegram start has no signed-in web session, so a vault account cannot be resolved and this is
/// empty. A **Local gh** project needs no vault: its identity is the host's own `gh` login, asked as
/// `os_user` — the same four pairs and `TDDY_GITHUB_SOURCE` a dashboard start carries.
pub(super) fn telegram_git_environment(
    project: &ProjectData,
    os_user: &str,
    github_cli: &tddy_host_service::GithubCli,
) -> Vec<(String, String)> {
    if !project.local_gh {
        return Vec::new();
    }
    SessionAccountAccess::none()
        .with_github_cli(github_cli.clone())
        .project_session_identity(
            // Names the start in the log lines the resolution writes; a Telegram start has no
            // session id yet.
            &format!("telegram start of project {}", project.project_id),
            Some(ProjectGithub::LocalGh {
                os_user,
                git_url: &project.git_url,
            }),
        )
        .git_environment
}

/// What a Telegram-started session's chat is told when its project acts as a GitHub account: that
/// the account's commit identity and the agent's GitHub tools are unavailable here, and why.
/// `None` for a project that assigns no account, or that uses Local gh — nothing is lost, so nothing is said.
pub(super) fn account_identity_unavailable_notice(project: &ProjectData) -> Option<String> {
    // A Local gh project's identity is the host's own `gh`, which needs no web session: nothing is
    // unavailable, so nothing is said.
    if project.local_gh {
        return None;
    }
    let account = project.accounts.first()?;
    Some(format!(
        "This project acts as the {} account `{}`, but a session started from Telegram has no \
         signed-in web session to open that account's credential vault with. Its commits use the \
         checkout's own git identity, and its agent's GitHub tools are unavailable. Start the \
         session from the web dashboard to act as the project's account.",
        capitalized_provider(&account.provider),
        account.account_id
    ))
}

fn capitalized_provider(provider: &str) -> String {
    match provider {
        "github" => "GitHub".to_string(),
        other => other.to_string(),
    }
}

impl<S: TelegramSender + Send + Sync> TelegramSessionControlHarness<S> {
    /// Tell the chat — and the log — that `project`'s account cannot act in a session started
    /// here. A project with no account assigned is not told anything.
    pub(super) async fn notify_account_identity_unavailable(
        &self,
        chat_id: i64,
        session_id: &str,
        project: &ProjectData,
    ) -> anyhow::Result<()> {
        let Some(notice) = account_identity_unavailable_notice(project) else {
            return Ok(());
        };
        log::warn!(
            target: "tddy_daemon::telegram_session_control",
            "session {session_id} (project {}) starts without its project's account identity: {notice}",
            project.project_id
        );
        self.sender.send_message(chat_id, &notice).await?;
        Ok(())
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

#[cfg(test)]
mod local_gh_tests {
    use std::collections::HashMap;

    use tddy_projects::project_storage::{AccountAssignment, ProjectData};

    use super::{account_identity_unavailable_notice, telegram_git_environment};

    fn a_local_gh_project_assigned_cloudflare() -> ProjectData {
        ProjectData {
            project_id: "11111111-2222-4333-8444-555555555555".to_string(),
            name: "alpha".to_string(),
            git_url: "https://github.com/acme/alpha.git".to_string(),
            main_repo_path: "/home/dev/repos/alpha".to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts: vec![AccountAssignment {
                provider: "cloudflare".to_string(),
                account_id: "acct-edge".to_string(),
            }],
            local_gh: true,
        }
    }

    #[test]
    fn a_telegram_start_of_a_local_gh_project_commits_as_the_hosts_gh_login() {
        // Given a Local gh project, and gh logged in to github.com as ada
        let dir = tempfile::tempdir().unwrap();
        let gh = tddy_testing_commons::gh_cli::a_gh_cli(dir.path())
            .logged_in_as("ada", 101)
            .build();
        let os_user = std::env::var("USER").expect("USER must be set");

        // When a Telegram start's identity is resolved
        let mut environment = telegram_git_environment(
            &a_local_gh_project_assigned_cloudflare(),
            &os_user,
            &tddy_host_service::GithubCli::at(gh),
        )
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>();
        environment.sort_unstable();

        // Then it acts as ada, told to ask gh, exactly as a dashboard start would
        assert_eq!(
            environment,
            [
                "GIT_AUTHOR_EMAIL=101+ada@users.noreply.github.com",
                "GIT_AUTHOR_NAME=ada",
                "GIT_COMMITTER_EMAIL=101+ada@users.noreply.github.com",
                "GIT_COMMITTER_NAME=ada",
                "TDDY_GITHUB_SOURCE=local-gh:github.com",
            ]
        );
    }

    #[test]
    fn a_telegram_start_of_a_local_gh_project_is_not_told_to_start_from_the_dashboard() {
        // Given a Local gh project that also assigns a Cloudflare account
        let project = a_local_gh_project_assigned_cloudflare();

        // When the chat would be told the project's account is unavailable
        let notice = account_identity_unavailable_notice(&project);

        // Then there is nothing to tell: gh is the identity, and needs no web session
        assert_eq!(notice, None);
    }
}
