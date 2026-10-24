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
use std::path::Path;

use tddy_supervisor::proto::supervisor::clone_event::Event;
use tddy_supervisor::proto::supervisor::{CloneExited, CloneRepositoryRequest};
use tddy_supervisor::request::{SessionState, SpawnSessionRequest};
use tddy_supervisor::SupervisorClient;

use crate::clone_run::CloneHandle;
use crate::git_progress::{parse, GitProgress, ProgressThrottle};

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

/// `request` with the commit identity a session is spawned under ([`SpawnOptions::git_environment`])
/// added to its `env`.
///
/// Like `PATH`, every one of these keys (the commit identity and `TDDY_GITHUB_SOURCE`) must be listed in the supervisor's
/// `spawn_policy.allowed_env_keys` or the spawn is refused outright — which is the point: a session
/// that silently started under the machine's identity instead of its project's account is the
/// failure this exists to prevent. A project that resolves to no account adds no key, so the shipped
/// empty policy still permits its spawn. Only [`spawner::SESSION_GIT_ENVIRONMENT_KEYS`] ever get
/// here ([`spawner::plan_session_child`] refuses the rest), so no token can ride this wire.
pub fn with_commit_identity(
    mut request: SpawnSessionRequest,
    git_environment: &[(String, String)],
) -> SpawnSessionRequest {
    request.env.extend(git_environment.iter().cloned());
    request
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
            provider_route_json: req.provider_route_json.as_deref(),
            mouse: req.mouse,
            recipe: req.recipe.as_deref(),
            stack_parent: req.stack_parent.as_deref(),
            stack_node_id: req.stack_node_id.as_deref(),
            stack_seed_base_session: req.stack_seed_base_session.as_deref(),
            model: req.model.as_deref(),
            host_session_socket: req.host_session_socket.as_deref(),
            git_environment: &req.git_environment,
        },
        &req.child_log_level,
        &req.child_log_format,
        req.coder_log_config_yaml.as_deref(),
    )?;

    let client = connect_supervisor(socket_path).await?;
    let request = with_commit_identity(
        spawn_session_request(
            &req.os_user,
            &plan.program,
            &plan.args,
            &plan.working_dir,
            plan.path_extra.as_deref(),
        ),
        &plan.env,
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

/// Clone a repository through the supervisor, as the target OS user, reporting nothing on the way.
///
/// An existing destination is left alone without asking the supervisor for anything, matching
/// [`crate::spawner::clone_as_user`].
pub async fn clone_repo_via_supervisor(
    socket_path: &Path,
    os_user: &str,
    git_url: &str,
    destination: &Path,
) -> anyhow::Result<()> {
    clone_repo_via_supervisor_with_progress(
        socket_path,
        os_user,
        git_url,
        destination,
        BTreeMap::new(),
        |_| {},
        |_| {},
    )
    .await
}

/// Clone `git_url` into `destination` as `os_user` through the supervisor at `socket_path`, with
/// `env` on the git child: `on_started` once git runs (its handle stops it via `StopSession`),
/// `on_progress` for each throttled progress line. Fails with git's own stderr tail.
///
/// One server-streaming `CloneRepository` call: the supervisor runs `git clone --progress` as the
/// user, into a temporary sibling of `destination` that it renames into place only on success.
/// `env` is checked against the supervisor's `spawn_policy.allowed_env_keys`, which **denies** a key
/// it does not list: an operator must list the keys a clone needs — `GIT_SSH_COMMAND`,
/// `TDDY_DAEMON_URL`, `TDDY_SESSION_TOKEN` for a facilitator-provisioned clone, and
/// `crate::clone_env::CloneSshEnv::ALLOWED_ENV_KEYS` for passphrase prompts. Git's own absolute path
/// must be in `allowed_tool_paths`, as it always was; the daemon cannot widen the policy.
///
/// Unbounded on purpose: how long a clone may take is the caller's deadline, not this function's to
/// guess. An existing destination is left alone without asking the supervisor for anything.
pub async fn clone_repo_via_supervisor_with_progress(
    socket_path: &Path,
    os_user: &str,
    git_url: &str,
    destination: &Path,
    env: BTreeMap<String, String>,
    on_started: impl FnOnce(CloneHandle),
    mut on_progress: impl FnMut(GitProgress),
) -> anyhow::Result<()> {
    if destination.exists() {
        log::info!(
            "supervisor_spawn: clone destination already exists, asking for nothing (dest={})",
            destination.display()
        );
        return Ok(());
    }

    let client = connect_supervisor(socket_path).await?;
    log::info!(
        "supervisor_spawn: asking {} to clone as os_user={} dest={}",
        socket_path.display(),
        os_user,
        destination.display(),
    );
    let mut events = client
        .clone_repository(CloneRepositoryRequest {
            os_user: os_user.to_string(),
            git_url: git_url.to_string(),
            destination: destination.display().to_string(),
            env: env.into_iter().collect(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("tddy-supervisor refused to clone the repository: {e}"))?;

    let mut on_started = Some(on_started);
    let mut throttle = ProgressThrottle::new();
    while let Some(event) = events.recv().await {
        let event = event.map_err(|e| {
            anyhow::anyhow!("tddy-supervisor's clone of the repository failed: {e}")
        })?;
        match event.event {
            Some(Event::Started(started)) => {
                if let Some(on_started) = on_started.take() {
                    on_started(CloneHandle::through_supervisor(socket_path, started.pid));
                }
            }
            Some(Event::Progress(progress)) => {
                if let Some(progress) = parse(&progress.line) {
                    if throttle.admit(&progress, std::time::Instant::now()) {
                        on_progress(progress);
                    }
                }
            }
            Some(Event::Exited(exited)) => return clone_outcome(&exited, destination),
            None => {}
        }
    }
    anyhow::bail!(
        "tddy-supervisor ended the clone's stream without saying how git exited (dest={})",
        destination.display()
    )
}

/// How git's exit reads as a result: success, or git's own words about why not.
fn clone_outcome(exited: &CloneExited, destination: &Path) -> anyhow::Result<()> {
    if exited.code == Some(0) {
        return Ok(());
    }
    let how = match exited.code {
        Some(code) => format!("exited with code {code}"),
        None => "was killed by a signal".to_string(),
    };
    if exited.stderr_tail.is_empty() {
        anyhow::bail!("git clone failed: {how} (dest={})", destination.display())
    }
    anyhow::bail!(
        "git clone failed: {how} (dest={}): {}",
        destination.display(),
        exited.stderr_tail
    )
}

/// Ask the supervisor to stop the clone it is running as `pid`, and wait for it to answer.
///
/// On a thread of its own with its own runtime, because [`CloneHandle::stop`] is synchronous and may
/// be called from inside the daemon's runtime, where blocking on a nested one would panic.
#[cfg(unix)]
pub(crate) fn stop_supervised_clone(socket_path: &Path, pid: u32) -> std::io::Result<()> {
    let socket_path = socket_path.to_path_buf();
    std::thread::spawn(move || -> anyhow::Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let client = connect_supervisor(&socket_path).await?;
            client
                .stop_session(pid)
                .await
                .map_err(|e| anyhow::anyhow!("tddy-supervisor could not stop clone {pid}: {e}"))?;
            Ok(())
        })
    })
    .join()
    .map_err(|_| std::io::Error::other("the thread stopping the clone panicked"))?
    .map_err(|e| std::io::Error::other(format!("{e:#}")))
}
