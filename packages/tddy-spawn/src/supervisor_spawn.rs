//! Starting sessions and cloning repositories through `tddy-supervisor`.
//!
//! On a supervised host the daemon owns no privilege: it decides *what* to run — that is
//! [`crate::spawner::plan_session_child`]'s job, shared with the forked spawn worker — and asks the
//! supervisor to run it as the session's own OS user. The supervisor decides nothing about the
//! child except whether it is allowed to exist.
//!
//! Two consequences shape this module:
//!
//! - **The daemon is not the child's parent.** It cannot `waitpid` a process it did not fork, so the
//!   startup watch every spawn does (did the tool survive its own argument parsing?) polls
//!   `SessionStatus` instead of `try_wait`, on the same schedule
//!   ([`crate::spawner::StartupWatch`], carried in the request).
//! - **There is no fallback.** A supervisor that cannot be reached, or that refuses a request, fails
//!   the operation. Spawning the session here instead would run it as the daemon's own user with no
//!   isolation — exactly the regression the supervisor exists to remove.
//!
//! A client is connected per operation rather than held open: the supervisor may restart under a
//! long-lived daemon, and a fresh connect is both cheaper than reconnect logic and honest about
//! whether the supervisor is reachable *now*.
//!
//! One difference from the forked backend an operator should expect: the supervisor gives a session
//! no inherited stdout/stderr, so a supervised session's *raw* output (an early panic, before the
//! logger exists) is not captured in `<repo>/tmp/logs/child/`. Its `log:` config is unchanged, so
//! everything the session logs still lands where it always did.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_supervisor::request::{SessionState, SpawnSessionRequest};
use tddy_supervisor::SupervisorClient;

use crate::spawn_worker::SpawnRequest;
use crate::spawner::{
    self, LiveKitCreds, SessionChildPlan, SpawnOptions, SpawnResult, StartupWatch,
};
use crate::supervisor_client::connect_supervisor;

/// The `SpawnSession` request that starts a planned session child.
///
/// `env` is empty unless the target user's `~/.tddy/config.yaml` declares a `spawn_path_extra`:
/// `SpawnPolicy::allowed_env_keys` **denies** a request naming a key it does not list rather than
/// dropping the key, and the shipped policy lists none. So an ordinary session asks for nothing and
/// is spawned with the environment root chose for it, while a user who asked for a `PATH` prefix
/// either gets it or has their spawn refused — never silently loses it. An operator granting
/// `spawn_path_extra` must therefore list `PATH` in the supervisor's `spawn_policy.allowed_env_keys`.
pub fn spawn_session_request(
    os_user: &str,
    program: &Path,
    args: &[String],
    working_dir: &Path,
    path_extra: Option<&str>,
) -> SpawnSessionRequest {
    let mut env = BTreeMap::new();
    if let Some(extra) = path_extra {
        env.insert(
            "PATH".to_string(),
            spawner::merge_spawn_child_path(Some(extra)),
        );
    }
    SpawnSessionRequest {
        os_user: os_user.to_string(),
        tool_path: program.to_path_buf(),
        args: args.to_vec(),
        env,
        working_dir: Some(working_dir.to_path_buf()),
        scope: None,
    }
}

/// The `SpawnSession` request that clones a repository as the target user.
///
/// `git` itself, not a shell running a `git` command line: a shell on
/// `SpawnPolicy::allowed_tool_paths` would grant the daemon arbitrary code execution as every
/// allowlisted session user, which is the one thing the allowlist exists to prevent. The operator
/// consequence is that the git binary's own absolute path must appear in `allowed_tool_paths`.
///
/// `working_dir` is left unset so the supervisor uses the target user's home; the destination is
/// absolute and `git clone` creates it.
pub fn clone_request(
    os_user: &str,
    git_program: &Path,
    git_url: &str,
    destination: &Path,
) -> SpawnSessionRequest {
    clone_request_with_env(os_user, git_program, git_url, destination, BTreeMap::new())
}

/// The `SpawnSession` request that clones a repository as the target user, carrying transport-
/// shim env vars the clone needs (PRD AC37).
///
/// `env` holds `GIT_SSH_COMMAND`, `TDDY_DAEMON_URL`, `TDDY_SESSION_TOKEN` for a facilitator-
/// provisioned clone that drives `tddy-remote-git-repo` as its `GIT_SSH_COMMAND`. The supervisor's
/// `SpawnPolicy::allowed_env_keys` **denies** any key not on its list, so an operator must add those
/// three keys to `spawn_policy.allowed_env_keys` in `supervisor.yaml` for facilitator clones to
/// work under a supervisor — the daemon cannot widen the policy itself, by design.
pub fn clone_request_with_env(
    os_user: &str,
    git_program: &Path,
    git_url: &str,
    destination: &Path,
    env: BTreeMap<String, String>,
) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: os_user.to_string(),
        tool_path: git_program.to_path_buf(),
        args: vec![
            "clone".to_string(),
            git_url.to_string(),
            destination.display().to_string(),
        ],
        env,
        working_dir: None,
        scope: None,
    }
}

/// The first executable named `program` on `path_var`, as an absolute path.
///
/// The supervisor is asked for an absolute program because its allowlist is a set of absolute paths;
/// resolving here means the binary an operator allowlists is the same one the daemon would have run
/// itself.
pub fn program_in_path(program: &str, path_var: &str) -> Option<PathBuf> {
    path_var
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(|entry| Path::new(entry).join(program))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// The `git` this daemon would run, named absolutely so the supervisor can allowlist it.
fn resolve_git_program() -> anyhow::Result<PathBuf> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    program_in_path("git", &path_var).ok_or_else(|| {
        anyhow::anyhow!(
            "no `git` on the daemon's PATH, so there is no clone tool to ask tddy-supervisor for \
             (PATH={path_var})"
        )
    })
}

/// Start a session through the supervisor, from the same request the forked spawn worker would get.
#[cfg(unix)]
pub async fn spawn_session_via_supervisor(
    socket_path: &Path,
    req: &SpawnRequest,
) -> anyhow::Result<SpawnResult> {
    let livekit = LiveKitCreds {
        url: req.livekit_url.clone(),
        api_key: req.livekit_api_key.clone(),
        api_secret: req.livekit_api_secret.clone(),
        common_room: req.common_room.clone(),
        daemon_instance_id: req.daemon_instance_id.clone(),
    };
    let plan = spawner::plan_session_child(
        &req.os_user,
        &req.tool_path,
        Path::new(&req.tddy_data_dir),
        Path::new(&req.repo_path),
        &livekit,
        SpawnOptions {
            resume_session_id: req.resume_session_id.as_deref(),
            new_session_id: req.new_session_id.as_deref(),
            project_id: req.project_id.as_deref(),
            agent: req.agent.as_deref(),
            agent_def_json: req.agent_def_json.as_deref(),
            mouse: req.mouse,
            recipe: req.recipe.as_deref(),
            stack_parent: req.stack_parent.as_deref(),
            stack_node_id: req.stack_node_id.as_deref(),
            stack_seed_base_session: req.stack_seed_base_session.as_deref(),
            model: req.model.as_deref(),
            host_session_socket: req.host_session_socket.as_deref(),
        },
        &req.child_log_level,
        &req.child_log_format,
        req.coder_log_config_yaml.as_deref(),
    )?;

    let client = connect_supervisor(socket_path).await?;
    let request = spawn_session_request(
        &req.os_user,
        &plan.program,
        &plan.args,
        &plan.working_dir,
        plan.path_extra.as_deref(),
    );
    log::info!(
        "supervisor_spawn: asking {} to spawn session_id={} tool={} os_user={} grpc_port={}",
        socket_path.display(),
        plan.session_id,
        plan.program.display(),
        req.os_user,
        plan.grpc_port
    );
    let spawned = client
        .spawn_session(request)
        .await
        .map_err(|e| anyhow::anyhow!("tddy-supervisor refused to spawn the session: {e}"))?;

    let startup =
        StartupWatch::from_millis(req.startup_grace_period_ms, req.startup_poll_interval_ms);
    watch_session_startup(&client, spawned.pid, &plan, startup).await?;

    log::info!(
        "supervisor_spawn: session_id={} pid={} livekit_room={} livekit_server_identity={}",
        plan.session_id,
        spawned.pid,
        plan.livekit_room,
        plan.livekit_server_identity
    );
    Ok(SpawnResult {
        session_id: plan.session_id,
        livekit_room: plan.livekit_room,
        livekit_server_identity: plan.livekit_server_identity,
        livekit_url: livekit.url,
        pid: spawned.pid,
        grpc_port: plan.grpc_port,
    })
}

/// Fail a spawn whose child exited during the startup grace period, as `spawn_as_user` does — the
/// alternative is reporting a session id that never becomes a session.
#[cfg(unix)]
async fn watch_session_startup(
    client: &SupervisorClient,
    pid: u32,
    plan: &SessionChildPlan,
    startup: StartupWatch,
) -> anyhow::Result<()> {
    let mut waited = std::time::Duration::ZERO;
    loop {
        let status = client
            .session_status(pid)
            .await
            .map_err(|e| anyhow::anyhow!("tddy-supervisor could not report session {pid}: {e}"))?;
        match status.state {
            SessionState::Exited => {
                log::warn!(
                    "supervisor_spawn: session exited during startup grace period session_id={} pid={} exit_code={:?}",
                    plan.session_id,
                    pid,
                    status.exit_code
                );
                return Err(anyhow::anyhow!(
                    "tddy-coder exited immediately after starting ({})",
                    match status.exit_code {
                        Some(code) => format!("exit code {code}"),
                        None => "killed by a signal".to_string(),
                    }
                ));
            }
            SessionState::Running if waited >= startup.grace => return Ok(()),
            SessionState::Running => {
                tokio::time::sleep(startup.poll).await;
                waited += startup.poll;
            }
        }
    }
}

/// Clone a repository through the supervisor, as the target OS user.
///
/// An existing destination is left alone without asking the supervisor for anything, matching
/// [`crate::spawner::clone_as_user`].
pub async fn clone_repo_via_supervisor(
    socket_path: &Path,
    os_user: &str,
    git_url: &str,
    destination: &Path,
) -> anyhow::Result<()> {
    clone_repo_via_supervisor_with_env(socket_path, os_user, git_url, destination, BTreeMap::new())
        .await
}

/// Clone a repository through the supervisor with transport-shim env vars the clone needs (PRD AC37).
///
/// See [`clone_request_with_env`] for the `allowed_env_keys` requirement.
pub async fn clone_repo_via_supervisor_with_env(
    socket_path: &Path,
    os_user: &str,
    git_url: &str,
    destination: &Path,
    env: BTreeMap<String, String>,
) -> anyhow::Result<()> {
    if destination.exists() {
        log::info!(
            "supervisor_spawn: clone destination already exists, asking for nothing (dest={})",
            destination.display()
        );
        return Ok(());
    }

    let git = resolve_git_program()?;
    let client = connect_supervisor(socket_path).await?;
    log::info!(
        "supervisor_spawn: asking {} to clone as os_user={} dest={} git={}",
        socket_path.display(),
        os_user,
        destination.display(),
        git.display()
    );
    let spawned = client
        .spawn_session(clone_request_with_env(
            os_user,
            &git,
            git_url,
            destination,
            env,
        ))
        .await
        .map_err(|e| anyhow::anyhow!("tddy-supervisor refused to clone the repository: {e}"))?;

    let exit_code = wait_for_exit(&client, spawned.pid).await?;
    match exit_code {
        Some(0) => Ok(()),
        Some(code) => Err(anyhow::anyhow!(
            "git clone failed: exited with code {code} (dest={})",
            destination.display()
        )),
        None => Err(anyhow::anyhow!(
            "git clone failed: killed by a signal (dest={})",
            destination.display()
        )),
    }
}

/// Poll a supervisor-spawned process until it exits, returning its exit code.
///
/// Unbounded on purpose: how long a clone may take is the caller's deadline
/// (`spawn_worker_request_timeout`), not this function's to guess.
async fn wait_for_exit(client: &SupervisorClient, pid: u32) -> anyhow::Result<Option<i32>> {
    /// How often a clone in progress is checked. Its own constant, not the spawn-startup knob: an
    /// operator retuning how long a *session* is watched for an early exit is saying nothing about
    /// how often a clone should be polled.
    const CLONE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

    loop {
        let status = client
            .session_status(pid)
            .await
            .map_err(|e| anyhow::anyhow!("tddy-supervisor could not report process {pid}: {e}"))?;
        match status.state {
            SessionState::Exited => return Ok(status.exit_code),
            SessionState::Running => tokio::time::sleep(CLONE_POLL_INTERVAL).await,
        }
    }
}
