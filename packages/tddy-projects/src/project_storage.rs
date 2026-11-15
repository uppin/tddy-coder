//! Per-user project registry (`~/.tddy/projects/projects.yaml`).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which account a project uses at one provider.
///
/// An account id is minted by the daemon holding the credential and is unique only **within its
/// provider**, so an assignment names both. It is a reference, never a credential: nothing here is
/// secret, and a row that names an account this host has never seen is a legible state rather than
/// a corrupt one — see `tddy_accounts::AccountResolution`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountAssignment {
    pub provider: String,
    pub account_id: String,
}

/// One project row stored in `projects.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    pub project_id: String,
    pub name: String,
    pub git_url: String,
    pub main_repo_path: String,
    /// Remote-tracking ref used as the integration base for worktrees (`origin/main`, `upstream/main`,
    /// etc.). A stored ref is authoritative; absent (legacy) rows resolve their default live from the
    /// repository — see [`effective_integration_base_ref_for_project`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_branch_ref: Option<String>,
    /// Default remote name for the project (`origin`, `upstream`, ...). When unset, the remote is
    /// detected from the main worktree's upstream tracking branch, falling back to `origin` only as
    /// the last resort. See [`effective_remote_name_for_project`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    /// Per-host (or per-daemon-instance) checkout paths for the same logical `project_id`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub host_repo_paths: HashMap<String, String>,
    /// Which account this project uses at each provider, at most one per provider. **Absent means
    /// unassigned, and unassigned resolves to nothing** — no caller's own login, no sole account in
    /// the vault. A row written by an older daemon reads as empty, exactly as the three optional
    /// fields above do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<AccountAssignment>,
    /// The project's sessions take their GitHub identity from the host's own GitHub CLI login, as
    /// the session's OS user, instead of from a vault account. A row with this set has no github
    /// entry in `accounts`. Absent reads as `false`, and `false` is never written.
    #[serde(default, skip_serializing_if = "is_false")]
    pub local_gh: bool,
    /// The session this project runs as its main (project-home session) on this host. Absent means
    /// main is not set up; a row written by an older daemon reads that way. The `home_agent` key
    /// such a row may carry is ignored on read and not written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_session_id: Option<String>,
    /// What a new orchestrator on this host starts with unless the operator changes it in the
    /// composer (`#orchestrators` 11/11). Absent means none saved: every control starts at the
    /// agent's own default. A row written by an older daemon reads that way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator_defaults: Option<OrchestratorDefaults>,
    /// The project's soft-delete tombstone and last restore, flattened onto the row
    /// (`deleted_at`, `deleted_by`, `restored_at`). Absent keys read as a live project that never
    /// saw either event, so a row an older daemon wrote loads unchanged; a live row writes none.
    #[serde(flatten, default)]
    pub lifecycle: ProjectLifecycle,
}

/// A project row's soft-delete state. Deleting keeps everything the project owns on disk — this
/// tombstone is the only thing that changes — and the newest delete or restore event across the
/// hosts owning a project decides whether the logical project is deleted ([`Self::newest`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectLifecycle {
    /// RFC 3339 time the project was soft-deleted. Present = deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// The GitHub user who deleted it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<String>,
    /// RFC 3339 time the project was last restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_at: Option<String>,
}

impl ProjectLifecycle {
    /// The lifecycle a `ProjectEntry` carries on the wire, where an empty string means absent.
    pub fn from_wire(deleted_at: &str, deleted_by: &str, restored_at: &str) -> Self {
        let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
        Self {
            deleted_at: some(deleted_at),
            deleted_by: some(deleted_by),
            restored_at: some(restored_at),
        }
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// When this row's newest event happened: its delete when deleted, else its restore. `None`
    /// for a row that never saw either. An unreadable stamp sorts before every readable one, so it
    /// still beats "never" but loses to any real event.
    fn last_event_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let stamp = if self.is_deleted() {
            self.deleted_at.as_deref()
        } else {
            self.restored_at.as_deref()
        }?;
        Some(
            chrono::DateTime::parse_from_rfc3339(stamp)
                .map(|t| t.with_timezone(&chrono::Utc))
                .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC),
        )
    }

    /// The lifecycle that wins among one logical project's rows on several hosts: the row with the
    /// newest delete or restore event. Any event beats a row that never saw one; a delete and a
    /// restore at the same instant resolve to deleted, the conservative answer.
    pub fn newest<'a>(rows: impl IntoIterator<Item = &'a ProjectLifecycle>) -> ProjectLifecycle {
        let mut winner: Option<&ProjectLifecycle> = None;
        for row in rows {
            let Some(at) = row.last_event_at() else {
                continue;
            };
            let replaces = match winner.and_then(ProjectLifecycle::last_event_at) {
                None => true,
                Some(best) => at > best || (at == best && row.is_deleted()),
            };
            if replaces {
                winner = Some(row);
            }
        }
        winner.cloned().unwrap_or_default()
    }
}

impl ProjectData {
    pub fn is_deleted(&self) -> bool {
        self.lifecycle.is_deleted()
    }
}

/// The defaults a project's orchestrator composer starts with on one host — saved from the
/// project's settings pane, never sent by the composer itself. Each field absent means "the
/// agent's own default"; an empty string is never stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrchestratorDefaults {
    /// A model id the agent's catalogue lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// One of the agent's served effort levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// A provider account the agent runs on, instead of its vendor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_account_id: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsFile {
    #[serde(default)]
    pub projects: Vec<ProjectData>,
}

const PROJECTS_FILENAME: &str = "projects.yaml";

fn projects_file_path(projects_dir: &Path) -> std::path::PathBuf {
    projects_dir.join(PROJECTS_FILENAME)
}

/// Read all projects. Returns empty vec if file is missing.
pub fn read_projects(projects_dir: &Path) -> anyhow::Result<Vec<ProjectData>> {
    let path = projects_file_path(projects_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
    let file: ProjectsFile = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("parse {}: {}", path.display(), e))?;
    Ok(file.projects)
}

/// Write the full project list (replace).
pub fn write_projects(projects_dir: &Path, projects: &[ProjectData]) -> anyhow::Result<()> {
    std::fs::create_dir_all(projects_dir)
        .map_err(|e| anyhow::anyhow!("create {}: {}", projects_dir.display(), e))?;
    let path = projects_file_path(projects_dir);
    let file = ProjectsFile {
        projects: projects.to_vec(),
    };
    let contents =
        serde_yaml::to_string(&file).map_err(|e| anyhow::anyhow!("serialize projects: {}", e))?;
    tddy_core::atomic_file::write_atomic(&path, contents)
        .map_err(|e| anyhow::anyhow!("write {}: {}", path.display(), e))
}

/// Append one project after reading existing.
pub fn add_project(projects_dir: &Path, project: ProjectData) -> anyhow::Result<()> {
    log::info!("add_project: project_id={}", project.project_id);
    if let Some(ref r) = project.main_branch_ref {
        tddy_core::validate_chain_pr_integration_base_ref(r)
            .map_err(|e| anyhow::anyhow!("invalid main_branch_ref: {}", e))?;
    }
    let mut projects = read_projects(projects_dir)?;
    projects.push(project);
    write_projects(projects_dir, &projects)
}

/// Append `project` only if its `project_id` is not already registered.
///
/// Returns `(row, created)`: when the id already exists, the existing row is returned unchanged and
/// `created` is `false` (no write). Otherwise the project is appended (validated like
/// [`add_project`]) and returned with `created = true`. Idempotency primitive for adding a project
/// to a host with a reused `project_id`.
pub fn add_or_get_project(
    projects_dir: &Path,
    project: ProjectData,
) -> anyhow::Result<(ProjectData, bool)> {
    if let Some(existing) = find_project(projects_dir, &project.project_id)? {
        log::info!(
            "add_or_get_project: project_id={} already present, returning existing row",
            existing.project_id
        );
        return Ok((existing, false));
    }
    add_project(projects_dir, project.clone())?;
    Ok((project, true))
}

/// Find project by id.
pub fn find_project(projects_dir: &Path, project_id: &str) -> anyhow::Result<Option<ProjectData>> {
    let projects = read_projects(projects_dir)?;
    Ok(projects.into_iter().find(|p| p.project_id == project_id))
}

/// Find a project the way a caller names one: by `project_id` first, then by `name`.
///
/// The id is tried first so a project whose *name* collides with another project's id still
/// resolves to the row the id identifies.
pub fn find_project_by_ref(
    projects_dir: &Path,
    project_ref: &str,
) -> anyhow::Result<Option<ProjectData>> {
    let projects = read_projects(projects_dir)?;
    Ok(projects
        .iter()
        .find(|p| p.project_id == project_ref)
        .or_else(|| projects.iter().find(|p| p.name == project_ref))
        .cloned())
}

/// Resolves the default remote name for a registered project.
///
/// Resolution order (no remote is assumed — `origin` is the last resort only):
/// 1. **Main worktree upstream** — `tddy_core::worktree::detect_default_remote_name(repo_root)`
///    reads `git rev-parse --abbrev-ref @{upstream}` and takes the segment before the first `/`.
///    This is the developer's actual remote and the most authoritative signal.
/// 2. **Project config** — [`ProjectData::remote_name`] when the main worktree has no upstream
///    (detached HEAD, fresh repo without upstream set).
/// 3. **`origin`** — last-resort fallback when neither signal is available.
pub fn effective_remote_name_for_project(
    projects_dir: &Path,
    project_id: &str,
    repo_root: &Path,
) -> anyhow::Result<String> {
    log::debug!(
        "effective_remote_name_for_project: project_id={} repo_root={}",
        project_id,
        repo_root.display()
    );
    if let Some(detected) = tddy_core::worktree::detect_default_remote_name(repo_root) {
        log::debug!(
            "effective_remote_name_for_project: project_id={} detected remote={}",
            project_id,
            detected
        );
        return Ok(detected);
    }
    let project = find_project(projects_dir, project_id)?
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    if let Some(configured) = project
        .remote_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        log::debug!(
            "effective_remote_name_for_project: project_id={} configured remote={}",
            project_id,
            configured
        );
        return Ok(configured.to_string());
    }
    log::debug!(
        "effective_remote_name_for_project: project_id={} falling back to origin",
        project_id
    );
    Ok("origin".to_string())
}

/// Resolves the git integration base ref for worktree setup for a registered project.
///
/// A stored [`ProjectData::main_branch_ref`] is authoritative (validated, no probe). Legacy rows
/// without a stored ref resolve their default **live** from the repository via
/// [`tddy_core::resolve_default_integration_base_ref_with_remote`], using the remote resolved by
/// [`effective_remote_name_for_project`] (main worktree → project config → `origin`). The probe is
/// legacy-only and loses effect once a default is stored.
pub fn effective_integration_base_ref_for_project(
    projects_dir: &Path,
    project_id: &str,
) -> anyhow::Result<String> {
    log::debug!(
        "effective_integration_base_ref_for_project: project_id={}",
        project_id
    );
    let project = find_project(projects_dir, project_id)?
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    match &project.main_branch_ref {
        Some(r) => {
            tddy_core::validate_chain_pr_integration_base_ref(r)
                .map_err(|e| anyhow::anyhow!("invalid main_branch_ref: {}", e))?;
            log::info!(
                "effective_integration_base_ref_for_project: project_id={} ref={}",
                project_id,
                r
            );
            Ok(r.clone())
        }
        None => {
            log::info!(
                "effective_integration_base_ref_for_project: project_id={} resolving live from repository",
                project_id
            );
            let repo_root = std::path::Path::new(&project.main_repo_path);
            let remote = effective_remote_name_for_project(projects_dir, project_id, repo_root)?;
            tddy_core::resolve_default_integration_base_ref_with_remote(repo_root, Some(&remote))
                .map_err(|e| {
                    anyhow::anyhow!(
                        "resolve default integration base ref for project {}: {}",
                        project_id,
                        e
                    )
                })
        }
    }
}

/// Sets (or replaces) the stored default integration base ref for `project_id`.
///
/// Validates `main_branch_ref` with [`tddy_core::validate_chain_pr_integration_base_ref`] **before**
/// touching the registry, so a rejected ref never mutates `projects.yaml`. Errors when `project_id`
/// is unknown.
pub fn set_project_default_branch(
    projects_dir: &Path,
    project_id: &str,
    main_branch_ref: &str,
) -> anyhow::Result<()> {
    log::info!(
        "set_project_default_branch: project_id={} main_branch_ref={}",
        project_id,
        main_branch_ref
    );
    tddy_core::validate_chain_pr_integration_base_ref(main_branch_ref)
        .map_err(|e| anyhow::anyhow!("invalid main_branch_ref: {}", e))?;
    let mut projects = read_projects(projects_dir)?;
    let project = projects
        .iter_mut()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    project.main_branch_ref = Some(main_branch_ref.to_string());
    write_projects(projects_dir, &projects)
}

/// A project asked to use local gh and a vault GitHub account at once.
///
/// Returned (inside the [`anyhow::Error`]) by [`set_project_accounts`] so a caller can tell this
/// refusal of the request from a failure of the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalGhWithGithubAccount;

impl std::fmt::Display for LocalGhWithGithubAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "a project that uses local gh is assigned no github account; turn local gh off to assign one",
        )
    }
}

impl std::error::Error for LocalGhWithGithubAccount {}

/// Replaces `project_id`'s whole account assignment set.
///
/// Replace, never merge: two people editing a project's accounts concurrently cannot interleave
/// into a set neither of them chose. An empty `accounts` clears every assignment, which is how a
/// project is returned to unassigned. Refuses two entries naming the same provider, and errors when
/// `project_id` is unknown.
///
/// `local_gh` is the project's whole Local gh setting, written in the same replace. A project that
/// uses local gh takes its GitHub identity from each host's `gh`, so `local_gh` together with a
/// `github` entry is refused and the row is left exactly as it was.
pub fn set_project_accounts(
    projects_dir: &Path,
    project_id: &str,
    accounts: &[AccountAssignment],
    local_gh: bool,
) -> anyhow::Result<()> {
    log::info!(
        "set_project_accounts: project_id={} accounts={}",
        project_id,
        accounts.len()
    );
    let mut seen = std::collections::HashSet::new();
    for a in accounts {
        if !seen.insert(a.provider.as_str()) {
            anyhow::bail!(
                "more than one account assigned for provider {}; a project uses one account per provider",
                a.provider
            );
        }
    }
    if local_gh && accounts.iter().any(|a| a.provider == "github") {
        return Err(LocalGhWithGithubAccount.into());
    }
    let mut projects = read_projects(projects_dir)?;
    let project = projects
        .iter_mut()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    project.accounts = accounts.to_vec();
    project.local_gh = local_gh;
    write_projects(projects_dir, &projects)
}

/// Records (`Some`) or clears (`None`) the session `project_id` runs as its main. Errors when
/// `project_id` is unknown.
pub fn set_project_home_session(
    projects_dir: &Path,
    project_id: &str,
    home_session_id: Option<&str>,
) -> anyhow::Result<()> {
    log::info!(
        "set_project_home_session: project_id={} home_session_id={}",
        project_id,
        home_session_id.unwrap_or_default()
    );
    let mut projects = read_projects(projects_dir)?;
    let project = projects
        .iter_mut()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    project.home_session_id = home_session_id.map(str::to_string);
    write_projects(projects_dir, &projects)
}

/// Saves (`Some`) or clears (`None`) the orchestrator defaults `project_id` has on this host,
/// replacing whatever was saved. A defaults value whose every field is absent is stored as none.
/// Errors when `project_id` is unknown, leaving the registry as it was.
pub fn set_project_orchestrator_defaults(
    projects_dir: &Path,
    project_id: &str,
    defaults: Option<&OrchestratorDefaults>,
) -> anyhow::Result<()> {
    let mut projects = read_projects(projects_dir)?;
    let project = projects
        .iter_mut()
        .find(|p| p.project_id == project_id)
        .ok_or_else(|| anyhow::anyhow!("unknown project: {}", project_id))?;
    project.orchestrator_defaults = defaults.and_then(|d| {
        (d.model.is_some() || d.effort.is_some() || d.provider_account_id.is_some())
            .then(|| d.clone())
    });
    write_projects(projects_dir, &projects)
}

/// The time a lifecycle event is stamped with: RFC 3339 in UTC, to the millisecond.
pub fn lifecycle_timestamp_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// A project name as `CreateProject` accepts one, trimmed: not empty, no `/`, no `..`. The name is
/// also the default clone directory, which is why it may not be a path.
pub fn validate_project_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("project name is required".to_string());
    }
    if name.contains('/') || name.contains("..") {
        return Err("invalid project name".to_string());
    }
    Ok(name)
}

/// Apply `change` to `project_id`'s row and write the registry when it reports a change. `None`
/// when the project is not registered here (nothing is written); otherwise the row as stored.
fn update_project_row(
    projects_dir: &Path,
    project_id: &str,
    change: impl FnOnce(&mut ProjectData) -> bool,
) -> anyhow::Result<Option<ProjectData>> {
    let mut projects = read_projects(projects_dir)?;
    let Some(project) = projects.iter_mut().find(|p| p.project_id == project_id) else {
        return Ok(None);
    };
    let changed = change(project);
    let row = project.clone();
    if changed {
        write_projects(projects_dir, &projects)?;
    }
    Ok(Some(row))
}

/// Rename `project_id`. Only `name` changes: the checkout path is stored, not derived from it.
/// `None` when it is not registered here.
pub fn rename_project(
    projects_dir: &Path,
    project_id: &str,
    name: &str,
) -> anyhow::Result<Option<ProjectData>> {
    log::info!("rename_project: project_id={project_id} name={name}");
    update_project_row(projects_dir, project_id, |p| {
        let changed = p.name != name;
        p.name = name.to_string();
        changed
    })
}

/// Tombstone `project_id` as deleted at `deleted_at` by `deleted_by`. Only the row changes —
/// nothing the project owns on disk is touched. An already deleted project keeps its first
/// tombstone. `None` when it is not registered here.
pub fn mark_project_deleted(
    projects_dir: &Path,
    project_id: &str,
    deleted_at: &str,
    deleted_by: &str,
) -> anyhow::Result<Option<ProjectData>> {
    log::info!("mark_project_deleted: project_id={project_id} deleted_by={deleted_by}");
    update_project_row(projects_dir, project_id, |p| {
        if p.is_deleted() {
            return false;
        }
        p.lifecycle.deleted_at = Some(deleted_at.to_string());
        p.lifecycle.deleted_by = Some(deleted_by.to_string());
        true
    })
}

/// Clear `project_id`'s tombstone, recording `restored_at`. A live project is left as it is.
/// `None` when it is not registered here.
pub fn mark_project_restored(
    projects_dir: &Path,
    project_id: &str,
    restored_at: &str,
) -> anyhow::Result<Option<ProjectData>> {
    log::info!("mark_project_restored: project_id={project_id}");
    update_project_row(projects_dir, project_id, |p| {
        if !p.is_deleted() {
            return false;
        }
        p.lifecycle = ProjectLifecycle {
            restored_at: Some(restored_at.to_string()),
            ..ProjectLifecycle::default()
        };
        true
    })
}

/// Adopt `winner` — another host's lifecycle for `project_id` — when it is newer than this row's,
/// as [`ProjectLifecycle::newest`] decides. `true` when the row changed; an unregistered project
/// changes nothing.
pub fn adopt_project_lifecycle(
    projects_dir: &Path,
    project_id: &str,
    winner: &ProjectLifecycle,
) -> anyhow::Result<bool> {
    let mut adopted = false;
    update_project_row(projects_dir, project_id, |p| {
        if &p.lifecycle == winner || ProjectLifecycle::newest([&p.lifecycle, winner]) != *winner {
            return false;
        }
        log::info!(
            "adopt_project_lifecycle: project_id={project_id} deleted={}",
            winner.is_deleted()
        );
        p.lifecycle = winner.clone();
        adopted = true;
        true
    })?;
    Ok(adopted)
}

/// A deleted project on this host for the same repository (`git_url`, compared trimmed) or the
/// same checkout (`main_repo_path`) — what creating that repository again restores.
pub fn find_deleted_project_matching(
    projects_dir: &Path,
    git_url: &str,
    main_repo_path: &str,
) -> anyhow::Result<Option<ProjectData>> {
    let git_url = git_url.trim();
    Ok(read_projects(projects_dir)?.into_iter().find(|p| {
        p.is_deleted()
            && ((!git_url.is_empty() && p.git_url.trim() == git_url)
                || (!main_repo_path.is_empty() && p.main_repo_path == main_repo_path))
    }))
}

/// Refuse new work — a session start, a resume, a project-home tool — on a deleted project,
/// naming it and saying how to bring it back.
pub fn refuse_if_deleted(project: &ProjectData) -> Result<(), tddy_rpc::Status> {
    if !project.is_deleted() {
        return Ok(());
    }
    Err(tddy_rpc::Status::failed_precondition(format!(
        "project {} ({}) is deleted; restore it to use it again",
        project.name, project.project_id
    )))
}

/// [`refuse_if_deleted`] for `project_id` as this host's registry holds it. A project this host
/// does not register is not refused here: whatever resolves it next says what is missing.
pub fn refuse_if_project_deleted(
    projects_dir: &Path,
    project_id: &str,
) -> Result<(), tddy_rpc::Status> {
    match find_project(projects_dir, project_id)
        .map_err(|e| tddy_rpc::Status::internal(format!("read project registry: {e}")))?
    {
        Some(project) => refuse_if_deleted(&project),
        None => Ok(()),
    }
}

/// Resolved `main_repo_path` for `project_id` on `host_key` (simulated host or daemon instance id).
///
/// Multi-host: returns [`ProjectData::host_repo_paths`]\[host_key] when non-empty, else
/// [`ProjectData::main_repo_path`].
pub fn main_repo_path_for_host(
    projects_dir: &Path,
    project_id: &str,
    host_key: &str,
) -> anyhow::Result<Option<String>> {
    let p = find_project(projects_dir, project_id)?;
    Ok(p.map(|p| {
        if let Some(path) = p.host_repo_paths.get(host_key) {
            if !path.trim().is_empty() {
                log::debug!(
                    "main_repo_path_for_host: host_repo_paths[{host_key}] project_id={}",
                    p.project_id
                );
                return path.clone();
            }
        }
        log::debug!(
            "main_repo_path_for_host: legacy main_repo_path project_id={} host_key={}",
            p.project_id,
            host_key
        );
        p.main_repo_path.clone()
    }))
}

#[cfg(test)]
mod per_host_path_unit_tests {
    use super::*;
    use std::collections::HashMap;

    /// Per-host map wins over legacy `main_repo_path` for distinct hosts.
    #[test]
    fn main_repo_path_for_host_returns_host_map_entry_not_only_legacy() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();
        let mut host_repo_paths = HashMap::new();
        host_repo_paths.insert("unit-host-x".to_string(), "/x/checkout".to_string());
        host_repo_paths.insert("unit-host-y".to_string(), "/y/checkout".to_string());
        let project = ProjectData {
            project_id: "p1".to_string(),
            name: "n".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: "/legacy".to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths,
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        };
        write_projects(&projects_dir, &[project]).unwrap();
        let px = main_repo_path_for_host(&projects_dir, "p1", "unit-host-x")
            .unwrap()
            .unwrap();
        let py = main_repo_path_for_host(&projects_dir, "p1", "unit-host-y")
            .unwrap()
            .unwrap();
        assert_ne!(
            px, py,
            "same project_id must resolve to different paths per host_key"
        );
        assert_eq!(px, "/x/checkout");
        assert_eq!(py, "/y/checkout");
    }
}

#[cfg(test)]
mod project_integration_base_acceptance_tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    /// Legacy `projects.yaml` without `main_branch_ref` resolves its default **live** from the
    /// repository (legacy-only probe), not from a hardcoded constant.
    #[test]
    fn legacy_project_without_base_ref_resolves_live_from_repository() {
        fn git(cwd: &Path, args: &[&str]) {
            let st = std::process::Command::new("git")
                .current_dir(cwd)
                .args(args)
                .status()
                .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
            assert!(st.success(), "git {args:?} failed in {cwd:?}");
        }

        let temp = tempfile::tempdir().unwrap();
        // A source repo whose only mainline branch is `main` (no `master`).
        let source = temp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-b", "main"]);
        git(&source, &["config", "user.email", "t@e.st"]);
        git(&source, &["config", "user.name", "t"]);
        fs::write(source.join("README.md"), "x\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "init"]);
        // A clone carrying remote-tracking `origin/*` refs.
        let clone = temp.path().join("clone");
        git(
            temp.path(),
            &["clone", source.to_str().unwrap(), clone.to_str().unwrap()],
        );

        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let yaml = format!(
            "projects:\n- project_id: \"p-legacy\"\n  name: \"n\"\n  git_url: \"{}\"\n  main_repo_path: \"{}\"\n",
            source.to_str().unwrap(),
            clone.to_str().unwrap()
        );
        fs::write(projects_file_path(&projects_dir), yaml).unwrap();

        let eff = effective_integration_base_ref_for_project(&projects_dir, "p-legacy").unwrap();
        assert_eq!(eff, "origin/main");
    }

    /// `effective_remote_name_for_project` resolves the remote from the main worktree's upstream
    /// tracking branch first — so a clone whose `master` tracks `upstream/master` returns `upstream`,
    /// not `origin`.
    #[test]
    fn effective_remote_name_prefers_the_main_worktree_upstream() {
        fn git(cwd: &Path, args: &[&str]) {
            let st = std::process::Command::new("git")
                .current_dir(cwd)
                .args(args)
                .status()
                .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
            assert!(st.success(), "git {args:?} failed in {cwd:?}");
        }

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-b", "master"]);
        git(&source, &["config", "user.email", "t@e.st"]);
        git(&source, &["config", "user.name", "t"]);
        fs::write(source.join("README.md"), "x\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "init"]);
        let clone = temp.path().join("clone");
        git(
            temp.path(),
            &["clone", source.to_str().unwrap(), clone.to_str().unwrap()],
        );
        // Retrack `master` onto a remote named `upstream` (a second remote pointing at the source).
        git(&clone, &["remote", "rename", "origin", "upstream"]);
        git(
            &clone,
            &["branch", "--set-upstream-to=upstream/master", "master"],
        );

        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let yaml = format!(
            "projects:\n- project_id: \"p-up\"\n  name: \"n\"\n  git_url: \"{}\"\n  main_repo_path: \"{}\"\n  remote_name: \"fork\"\n",
            source.to_str().unwrap(),
            clone.to_str().unwrap()
        );
        fs::write(projects_file_path(&projects_dir), yaml).unwrap();

        // Given — main worktree upstream is `upstream`, project config says `fork`
        // When
        let remote = effective_remote_name_for_project(&projects_dir, "p-up", &clone).unwrap();

        // Then — the main worktree upstream wins over the project config
        assert_eq!(remote, "upstream");
    }

    /// With no main-worktree upstream (detached HEAD), `effective_remote_name_for_project` falls
    /// back to the project config's `remote_name`.
    #[test]
    fn effective_remote_name_falls_back_to_project_config_when_no_upstream() {
        fn git(cwd: &Path, args: &[&str]) {
            let st = std::process::Command::new("git")
                .current_dir(cwd)
                .args(args)
                .status()
                .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
            assert!(st.success(), "git {args:?} failed in {cwd:?}");
        }

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-b", "main"]);
        git(&source, &["config", "user.email", "t@e.st"]);
        git(&source, &["config", "user.name", "t"]);
        fs::write(source.join("README.md"), "x\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "init"]);
        let clone = temp.path().join("clone");
        git(
            temp.path(),
            &["clone", source.to_str().unwrap(), clone.to_str().unwrap()],
        );
        // Detach HEAD so `@{upstream}` is unreadable — config must take over.
        git(&clone, &["checkout", "--detach", "main"]);

        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let yaml = format!(
            "projects:\n- project_id: \"p-cfg\"\n  name: \"n\"\n  git_url: \"{}\"\n  main_repo_path: \"{}\"\n  remote_name: \"fork\"\n",
            source.to_str().unwrap(),
            clone.to_str().unwrap()
        );
        fs::write(projects_file_path(&projects_dir), yaml).unwrap();

        // Given — no upstream; project config names `fork`
        // When
        let remote = effective_remote_name_for_project(&projects_dir, "p-cfg", &clone).unwrap();

        // Then — the project config supplies the remote
        assert_eq!(remote, "fork");
    }

    /// With neither a main-worktree upstream nor a project config `remote_name`,
    /// `effective_remote_name_for_project` falls back to `origin` as the last resort.
    #[test]
    fn effective_remote_name_falls_back_to_origin_when_neither_signal_present() {
        fn git(cwd: &Path, args: &[&str]) {
            let st = std::process::Command::new("git")
                .current_dir(cwd)
                .args(args)
                .status()
                .unwrap_or_else(|e| panic!("git {args:?} in {cwd:?}: {e}"));
            assert!(st.success(), "git {args:?} failed in {cwd:?}");
        }

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "-b", "main"]);
        git(&source, &["config", "user.email", "t@e.st"]);
        git(&source, &["config", "user.name", "t"]);
        fs::write(source.join("README.md"), "x\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "-m", "init"]);
        let clone = temp.path().join("clone");
        git(
            temp.path(),
            &["clone", source.to_str().unwrap(), clone.to_str().unwrap()],
        );
        git(&clone, &["checkout", "--detach", "main"]);

        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let yaml = format!(
            "projects:\n- project_id: \"p-fb\"\n  name: \"n\"\n  git_url: \"{}\"\n  main_repo_path: \"{}\"\n",
            source.to_str().unwrap(),
            clone.to_str().unwrap()
        );
        fs::write(projects_file_path(&projects_dir), yaml).unwrap();

        // Given — no upstream and no `remote_name` in config
        // When
        let remote = effective_remote_name_for_project(&projects_dir, "p-fb", &clone).unwrap();

        // Then — `origin` is the last-resort fallback
        assert_eq!(remote, "origin");
    }

    /// Unsafe `main_branch_ref` values (shell metacharacters) must be rejected before YAML mutation.
    ///
    /// Note: under the remote-agnostic contract the validator no longer enforces a specific remote
    /// name, so a syntactically valid `<remote>/<path>` whose remote does not exist (e.g.
    /// `refs/heads/main`) is **accepted** at the boundary and rejected later by `git fetch`. The
    /// boundary guard rejects only unsafe strings — forbidden characters, `..`, `--`, whitespace.
    #[test]
    fn unsafe_base_ref_rejected_at_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let project = ProjectData {
            project_id: "bad-ref".to_string(),
            name: "n".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: "/tmp/r".to_string(),
            main_branch_ref: Some("upstream/main;rm -rf /".to_string()),
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        };
        let r = add_project(&projects_dir, project);
        assert!(
            r.is_err(),
            "unsafe integration base ref must be rejected before persistence: {:?}",
            r
        );
        assert!(
            read_projects(&projects_dir).unwrap().is_empty(),
            "projects.yaml must not be written when validation fails"
        );
    }

    /// A syntactically valid `<remote>/<path>` whose remote does not exist is accepted at the
    /// boundary (no git probe) — the guardrail is `git fetch` at session time, not the validator.
    #[test]
    fn non_origin_remote_ref_accepted_at_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let projects_dir = temp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        let project = ProjectData {
            project_id: "non-origin".to_string(),
            name: "n".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: "/tmp/r".to_string(),
            main_branch_ref: Some("upstream/release/2025".to_string()),
            remote_name: None,
            host_repo_paths: HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: Default::default(),
        };
        add_project(&projects_dir, project).expect(
            "a safe <remote>/<path> must be accepted at the boundary regardless of the remote name",
        );
        let stored = find_project(&projects_dir, "non-origin").unwrap().unwrap();
        assert_eq!(
            stored.main_branch_ref.as_deref(),
            Some("upstream/release/2025"),
            "the non-origin remote-tracking ref must persist verbatim"
        );
    }
}

#[cfg(test)]
mod set_project_default_branch_unit_tests {
    use super::*;
    use std::collections::HashMap;

    fn a_projects_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("projects");
        std::fs::create_dir_all(&dir).unwrap();
        (temp, dir)
    }

    fn given_a_project(projects_dir: &Path, project_id: &str, main_branch_ref: Option<&str>) {
        add_project(
            projects_dir,
            ProjectData {
                project_id: project_id.to_string(),
                name: "alpha".to_string(),
                git_url: "https://example.com/a.git".to_string(),
                main_repo_path: "/tmp/a".to_string(),
                main_branch_ref: main_branch_ref.map(str::to_string),
                remote_name: None,
                host_repo_paths: HashMap::new(),
                accounts: Vec::new(),
                local_gh: false,
                home_session_id: None,
                orchestrator_defaults: None,
                lifecycle: Default::default(),
            },
        )
        .expect("seed project");
    }

    #[test]
    fn set_updates_the_row_default_branch() {
        // Given a legacy project with no stored default
        let (_keep, dir) = a_projects_dir();
        given_a_project(&dir, "p1", None);

        // When
        set_project_default_branch(&dir, "p1", "origin/main").expect("set succeeds");

        // Then
        let stored = find_project(&dir, "p1").unwrap().unwrap();
        assert_eq!(stored.main_branch_ref.as_deref(), Some("origin/main"));
    }

    #[test]
    fn set_accepts_a_multi_segment_remote_branch() {
        // Given
        let (_keep, dir) = a_projects_dir();
        given_a_project(&dir, "p1", None);

        // When
        set_project_default_branch(&dir, "p1", "origin/release/2025").expect("set succeeds");

        // Then
        let stored = find_project(&dir, "p1").unwrap().unwrap();
        assert_eq!(
            stored.main_branch_ref.as_deref(),
            Some("origin/release/2025")
        );
    }

    #[test]
    fn set_rejects_an_unsafe_ref_without_mutating_the_row() {
        // Given a project that already has a default
        let (_keep, dir) = a_projects_dir();
        given_a_project(&dir, "p1", Some("origin/main"));

        // When
        let result = set_project_default_branch(&dir, "p1", "origin/main;rm -rf /");

        // Then — rejected and the previous default is untouched
        assert!(result.is_err(), "unsafe ref must be rejected");
        let stored = find_project(&dir, "p1").unwrap().unwrap();
        assert_eq!(stored.main_branch_ref.as_deref(), Some("origin/main"));
    }

    #[test]
    fn set_errors_on_an_unknown_project() {
        // Given an empty registry
        let (_keep, dir) = a_projects_dir();

        // When / Then
        assert!(
            set_project_default_branch(&dir, "missing", "origin/main").is_err(),
            "setting a default on an unknown project must be an error"
        );
    }
}
