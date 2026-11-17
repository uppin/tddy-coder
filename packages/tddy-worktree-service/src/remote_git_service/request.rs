use std::path::{Path, PathBuf};
use tddy_core::shell_single_quote;
use tddy_daemon_kernel::privilege_drop::ResolvedPtyUser;
use tddy_rpc::Status;
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen};

/// The git pack verbs this service will run. A closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVerb {
    UploadPack,
    ReceivePack,
}

impl GitVerb {
    /// The git subcommand pair this verb execs, e.g. `("git", "upload-pack")`.
    pub fn git_subcommand(self) -> &'static str {
        match self {
            GitVerb::UploadPack => "upload-pack",
            GitVerb::ReceivePack => "receive-pack",
        }
    }
}

/// Read the `open` payload from a `Serve` stream's first frame.
///
/// Only `open` is read here. The frame's `stdin`/`stdin_eof` are the caller's to relay — `serve`
/// takes them before calling this and applies them to the child once it exists, so a client that
/// sends payload alongside its open loses nothing.
pub fn open_from_first_frame(frame: GitClientFrame) -> Result<GitOpen, Status> {
    frame.open.ok_or_else(|| {
        Status::invalid_argument("the first frame of a Serve stream must carry `open`")
    })
}

/// Resolve a wire `verb` against the closed whitelist. Accepts both spellings git uses
/// (`git-upload-pack` and `git upload-pack`).
pub fn resolve_git_verb(verb: &str) -> Result<GitVerb, Status> {
    match verb {
        "git-upload-pack" | "git upload-pack" => Ok(GitVerb::UploadPack),
        "git-receive-pack" | "git receive-pack" => Ok(GitVerb::ReceivePack),
        other => Err(Status::permission_denied(format!(
            "'{other}' is not a git pack verb; only git-upload-pack and git-receive-pack are served"
        ))),
    }
}

/// Resolve `project_ref` to the project's `main_repo_path`, matching `project_id` first and `name`
/// second. The returned path always comes from the registry — never from `project_ref`.
pub fn resolve_project_repo(projects_dir: &Path, project_ref: &str) -> Result<PathBuf, Status> {
    let project = crate::project_storage::find_project_by_ref(projects_dir, project_ref)
        .map_err(|e| Status::internal(format!("read project registry: {e}")))?
        .ok_or_else(|| Status::not_found(format!("no project '{project_ref}'")))?;
    let repo_path = PathBuf::from(&project.main_repo_path);
    if !repo_path.exists() {
        return Err(Status::failed_precondition(format!(
            "project '{}' points at {}, which does not exist",
            project.name,
            repo_path.display()
        )));
    }
    Ok(repo_path)
}

/// Where a Serve spawn runs pack verbs. Local is today's `main_repo_path`. Remote streams the
/// verb through the remote-shell port (`tddy_remote_shell::RemoteShell`, its ssh transport) —
/// this crate cannot take `tddy_tool_engine::Shell` because that crate already depends on this
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackExecution {
    Local {
        repo_path: PathBuf,
    },
    Remote {
        ssh_config_host: String,
        remote_repo_path: String,
    },
}

/// Empty `ssh_config_host` is LocalShell. A set alias packs on the SSH target, never locally.
pub fn pack_execution_for_session(
    local_repo: PathBuf,
    ssh_config_host: &str,
    remote_repo_path: &str,
) -> PackExecution {
    if ssh_config_host.is_empty() {
        PackExecution::Local {
            repo_path: local_repo,
        }
    } else {
        PackExecution::Remote {
            ssh_config_host: ssh_config_host.to_string(),
            remote_repo_path: remote_repo_path.to_string(),
        }
    }
}

/// The argv the child is spawned with, front-loaded with a `setpriv` privilege drop when the
/// target OS user differs from the daemon's own identity (reusing
/// [`tddy_daemon_kernel::privilege_drop::wrap_argv_for_privilege_drop`], so the PTY and pipe paths cannot diverge).
pub fn git_argv(
    verb: GitVerb,
    repo_path: &Path,
    os_user: Option<&str>,
) -> Result<Vec<String>, Status> {
    match os_user {
        None => Ok(git_child_argv(verb, repo_path)),
        Some(os_user) => Ok(git_child_command(verb, repo_path, os_user)?.argv),
    }
}

/// The argv and environment a git child for `os_user` is spawned with, resolved together so the
/// passwd lookup happens once.
pub fn git_child_command(
    verb: GitVerb,
    repo_path: &Path,
    os_user: &str,
) -> Result<GitChildCommand, Status> {
    let target = tddy_daemon_kernel::privilege_drop::resolve_pty_os_user(os_user)
        .map_err(|e| Status::internal(format!("cannot resolve os_user '{os_user}': {e}")))?;
    let (current_uid, current_gid) = daemon_identity();
    Ok(GitChildCommand {
        argv: git_argv_as_user(verb, repo_path, &target, current_uid, current_gid),
        env: git_child_env(&target),
    })
}

/// The command line and environment of one git child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChildCommand {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The argv for a git child that must run as the already-resolved `target` user, given the identity
/// the daemon itself runs under.
///
/// Split out from [`git_argv`] so the privilege drop can be exercised with ids the test host does
/// not have to own: the passwd lookup is the only part that needs a real account.
pub fn git_argv_as_user(
    verb: GitVerb,
    repo_path: &Path,
    target: &ResolvedPtyUser,
    current_uid: u32,
    current_gid: u32,
) -> Vec<String> {
    let argv = git_child_argv(verb, repo_path);
    if tddy_daemon_kernel::privilege_drop::pty_requires_privilege_drop(
        target.uid,
        target.gid,
        current_uid,
        current_gid,
    ) {
        tddy_daemon_kernel::privilege_drop::wrap_argv_for_privilege_drop(
            &argv, target.uid, target.gid,
        )
    } else {
        argv
    }
}

/// The environment a git child runs with, and the only environment it gets: it is spawned with the
/// daemon's own environment cleared.
///
/// `setpriv` preserves the environment across the uid boundary, so inheriting would hand a process
/// running as somebody else every variable the daemon was started with — including
/// `LIVEKIT_API_SECRET`, with which anybody can mint themselves into any room as any identity.
/// `git receive-pack` runs the repository's hooks and `git upload-pack` honours
/// `uploadpack.packObjectsHook`, so that environment is reachable by repository-controlled code.
/// Inheriting `HOME` is wrong for a second reason: git would read the daemon's `.gitconfig` instead
/// of the project owner's.
fn git_child_env(target: &ResolvedPtyUser) -> Vec<(String, String)> {
    let home = PathBuf::from(&target.home_dir);
    let path_extra = tddy_daemon_kernel::user_paths::spawn_path_extra_for_home(&home);
    tddy_daemon_kernel::privilege_drop::pty_user_env_overrides(&home, path_extra.as_deref())
}

/// Shell command run on the SSH target for a pack verb. The repo path in `git_argv` is replaced
/// with `remote_repo_path` so the child never packs a path the session did not authorize.
pub(super) fn remote_pack_shell_command(
    git_argv: &[String],
    remote_repo_path: &str,
) -> Result<String, String> {
    let verb = git_argv
        .get(1)
        .ok_or_else(|| "pack argv must include a git subcommand".to_string())?;
    if git_argv.first().map(String::as_str) != Some("git") {
        return Err("pack argv must start with git".to_string());
    }
    Ok(format!(
        "git {} -- {}",
        verb,
        shell_single_quote(remote_repo_path)
    ))
}

/// `git <subcommand> -- <repo>`. The `--` matters because `main_repo_path` comes from a
/// hand-editable `projects.yaml`: a path beginning with `-` would otherwise be parsed as an option.
fn git_child_argv(verb: GitVerb, repo_path: &Path) -> Vec<String> {
    vec![
        "git".to_string(),
        verb.git_subcommand().to_string(),
        "--".to_string(),
        repo_path.to_string_lossy().into_owned(),
    ]
}

/// The uid/gid the daemon process itself runs under.
fn daemon_identity() -> (u32, u32) {
    unsafe { (libc::getuid(), libc::getgid()) }
}
