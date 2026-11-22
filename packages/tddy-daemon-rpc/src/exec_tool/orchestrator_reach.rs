//! An orchestrator session's read reach, built per call where exec tools run (`#orchestra` 1/9).
//!
//! `Read`, `Grep` and `Glob` of a session whose recorded role is an orchestrator's are answered from
//! a [`ReadReach`]: its own worktree, every worktree of every tddy project on this host, the
//! home and chat directories each coding agent declares (`coding_agent.home_dirs`) under the OS
//! user's home and under the agent's own home, and the paths the operator granted the session. Any
//! other session's call is not answered here and takes the ordinary worktree-confined route.
//!
//! In another project's worktree only what git lists is readable (`.git` and git-ignored paths
//! stay hidden); agent homes and grants are read as-is.
//!
//! The reach is rebuilt for every call, so a project, a worktree or a grant added since the last
//! call is in it, and nothing is cached that could outlive what it describes.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::{Path, PathBuf};

use tddy_actions::tool::HomeSection;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::is_orchestrator_role;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::{home_dir_for_user, projects_path_for_user};
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::path_grants::{PathGrantMode, PathGrantStore};
use tddy_tool_engine::reach::{execute_in_reach, is_reach_tool, ReadReach};

use super::project_home::caller::record_call;
use super::ExecToolRpcHandler;

/// The tools an orchestrator may run inside a write grant, and nowhere else.
pub(super) fn is_granted_write_tool(tool_name: &str) -> bool {
    matches!(tool_name, "Write" | "StrReplace" | "Delete")
}

/// A write that lies inside one of a session's write grants (`#orchestra` 6/9).
pub(super) struct GrantedWrite {
    /// The grant the path is inside, symlinks resolved: where the write is confined.
    pub(super) root: PathBuf,
    /// The path to write, absolute and resolved like a read's.
    pub(super) path: PathBuf,
}

/// The write grant `args_json`'s `path` lies inside, for the session whose directory is
/// `session_dir`: `None` for a relative path (the home worktree is never writable), a path outside
/// every write grant, and a read-only grant. The path is resolved the way a read's is
/// ([`ReadReach::resolve`]): symlinks followed, `..` refused, a path not yet created judged by the
/// directory it would be created in.
pub(super) fn granted_write(session_dir: &Path, args_json: &str) -> Option<GrantedWrite> {
    let args: serde_json::Value = serde_json::from_str(args_json).ok()?;
    granted_write_path(session_dir, args.get("path")?.as_str()?)
}

/// The write grant `path` lies inside, as [`granted_write`] judges a tool's `path` argument.
pub(super) fn granted_write_path(session_dir: &Path, path: &str) -> Option<GrantedWrite> {
    if !Path::new(path).is_absolute() {
        return None;
    }
    let roots: Vec<PathBuf> = PathGrantStore::open(session_dir)
        .grants()
        .ok()?
        .into_iter()
        .filter(|grant| grant.mode == PathGrantMode::Write)
        .map(|grant| grant.path)
        .collect();
    // The session directory stands in for the home, which the reach always contains: the check
    // that decides is the write roots below, never the reach.
    let reach = ReadReach::new(session_dir.to_path_buf(), roots);
    let resolved = reach.resolve(path).ok()?;
    let root = reach
        .roots()
        .iter()
        .find(|root| resolved.starts_with(root))?
        .clone();
    Some(GrantedWrite {
        root,
        path: resolved,
    })
}

/// Whether the session whose directory is `session_dir` records an orchestrator role; an unreadable
/// record is not one.
fn is_orchestrator_session(session_dir: &Path) -> bool {
    tddy_core::read_session_metadata(session_dir)
        .ok()
        .and_then(|metadata| metadata.session_role)
        .as_deref()
        .is_some_and(is_orchestrator_role)
}

/// One coding agent's contribution to a reach: its declared directories, and the home they are
/// relative to besides the OS user's.
struct AgentHomes {
    home_dirs: Vec<String>,
    /// The agent's own home when it has one that outlives a session.
    own_home: Option<PathBuf>,
}

/// What building a reach reads, gathered up front so the blocking part runs off the async runtime.
struct ReachInputs {
    home_worktree: PathBuf,
    session_dir: PathBuf,
    projects_dir: Option<PathBuf>,
    local_instance_id: String,
    os_user_home: Option<PathBuf>,
    agents: Vec<AgentHomes>,
}

impl ExecToolRpcHandler {
    /// Answer `req` from its caller's [`ReadReach`] when the caller is an orchestrator and the tool
    /// is one a reach answers; `None` when the call is not this module's.
    pub(super) async fn run_in_orchestrator_reach(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        if !is_reach_tool(&req.tool_name) || !req.conversation_id.is_empty() {
            return Ok(None);
        }
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        if !is_orchestrator_session(&session_dir) {
            return Ok(None);
        }

        let reach = self
            .orchestrator_read_reach(&req.session_token, session_dir, worktree_root)
            .await?;
        let outcome = execute_in_reach(&reach, &req.tool_name, &req.args_json).await;
        let response = ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        };
        record_call(sessions_base, req, &response);
        Ok(Some(response))
    }

    /// The [`ReadReach`] of the orchestrator session whose directory is `session_dir` and whose home
    /// worktree is `worktree_root`, as it stands now (projects, worktrees and grants re-read).
    pub(super) async fn orchestrator_read_reach(
        &self,
        session_token: &str,
        session_dir: PathBuf,
        worktree_root: &Path,
    ) -> Result<ReadReach, Status> {
        let os_user = self.os_user_of(session_token)?;
        let inputs = ReachInputs {
            home_worktree: worktree_root.to_path_buf(),
            session_dir,
            projects_dir: projects_path_for_user(&os_user, Some(&self.tddy_data_dir)),
            local_instance_id: local_instance_id_for_config(&self.config),
            os_user_home: home_dir_for_user(&os_user),
            agents: self.agent_homes(),
        };
        tokio::task::spawn_blocking(move || build_reach(inputs))
            .await
            .map_err(|e| Status::internal(format!("building the read reach failed: {e}")))?
            .map_err(|e| Status::internal(format!("building the read reach failed: {e}")))
    }

    /// Run an orchestrator's `Write`, `StrReplace` or `Delete` that lies inside one of its write
    /// grants, confined to that grant; `None` when the call is not that (another role, another
    /// tool, a subagent conversation, or a path outside every write grant), which the role guard
    /// has refused for an orchestrator already.
    pub(super) async fn run_granted_write(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        if !is_granted_write_tool(&req.tool_name) || !req.conversation_id.is_empty() {
            return Ok(None);
        }
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        if !is_orchestrator_session(&session_dir) {
            return Ok(None);
        }
        let Some(granted) = granted_write(&session_dir, &req.args_json) else {
            return Ok(None);
        };
        let mut args: serde_json::Value = serde_json::from_str(&req.args_json)
            .map_err(|e| Status::invalid_argument(format!("invalid args_json: {e}")))?;
        args["path"] = serde_json::Value::String(granted.path.display().to_string());
        let confined = ExecuteToolRequest {
            args_json: args.to_string(),
            ..req.clone()
        };
        let response = self
            .local_exec_tools
            .run_exec_tool_locally(&confined, sessions_base, &granted.root)
            .await;
        record_call(sessions_base, req, &response);
        Ok(Some(response))
    }

    /// Every coding agent this host launches that declares `home_dirs`, with the home they are
    /// relative to besides the OS user's.
    fn agent_homes(&self) -> Vec<AgentHomes> {
        let Some(launch) = self.session_launch.get() else {
            return Vec::new();
        };
        let registry = launch.coding_agents.agents();
        registry
            .all()
            .into_iter()
            .filter(|spec| !spec.home_dirs().is_empty())
            .map(|spec| AgentHomes {
                home_dirs: spec.home_dirs().to_vec(),
                own_home: registry
                    .tools()
                    .get(&spec.id)
                    .and_then(|def| match &def.home {
                        HomeSection::Persistent { key } => {
                            Some(self.tddy_data_dir.join("homes").join(key))
                        }
                        HomeSection::Path { path } => Some(path.clone()),
                        HomeSection::Session => None,
                    }),
            })
            .collect()
    }
}

fn build_reach(inputs: ReachInputs) -> Result<ReadReach, String> {
    let home = inputs.home_worktree.canonicalize().map_err(|e| {
        format!(
            "cannot canonicalize {}: {e}",
            inputs.home_worktree.display()
        )
    })?;
    let worktrees = project_worktrees(&inputs);
    let mut roots = Vec::new();
    for agent in &inputs.agents {
        for home_dir in &agent.home_dirs {
            roots.extend(inputs.os_user_home.iter().map(|h| h.join(home_dir)));
            roots.extend(agent.own_home.iter().map(|h| h.join(home_dir)));
        }
    }
    let grants = PathGrantStore::open(&inputs.session_dir)
        .grants()
        .map_err(|e| format!("cannot read the session's path grants: {e}"))?;
    roots.extend(grants.into_iter().map(|grant| grant.path));
    Ok(ReadReach::new(home, roots).with_worktrees(worktrees))
}

/// Every worktree of every project registered on this host, each project's main checkout included.
/// A project whose repository is missing or unreadable contributes nothing.
fn project_worktrees(inputs: &ReachInputs) -> Vec<PathBuf> {
    let Some(projects_dir) = &inputs.projects_dir else {
        return Vec::new();
    };
    let projects = match tddy_projects::project_storage::read_projects(projects_dir) {
        Ok(projects) => projects,
        Err(e) => {
            log::warn!("orchestrator reach: cannot read the project list: {e}");
            return Vec::new();
        }
    };
    projects
        .iter()
        .filter_map(|project| {
            tddy_projects::project_storage::main_repo_path_for_host(
                projects_dir,
                &project.project_id,
                &inputs.local_instance_id,
            )
            .ok()
            .flatten()
        })
        .flat_map(|main_repo| registered_worktrees(Path::new(&main_repo)))
        .collect()
}

/// The paths `git worktree list` reports for `repo` (its own checkout first).
fn registered_worktrees(repo: &Path) -> Vec<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(|path| PathBuf::from(path.trim()))
            .collect(),
        Ok(out) => {
            log::warn!(
                "orchestrator reach: git worktree list failed in {}: {}",
                repo.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
            Vec::new()
        }
        Err(e) => {
            log::warn!(
                "orchestrator reach: cannot run git in {}: {e}",
                repo.display()
            );
            Vec::new()
        }
    }
}
