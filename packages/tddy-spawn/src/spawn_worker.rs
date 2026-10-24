//! Spawn worker — child process that performs fork+exec for the daemon, serving each request on a
//! thread of its own.
//!
//! Fork from a multi-threaded tokio process can deadlock (pthread/malloc locks).
//! We fork this worker before tokio starts, so the forking thread is the only one it inherits and
//! nothing it needs from tokio can be held by a thread that did not survive the fork.

use std::os::unix::io::FromRawFd;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::spawner::{self, LiveKitCreds, SpawnOptions, SpawnResult};

mod clone_request;
pub use clone_request::CloneRequest;
mod client;
pub use client::{client_over, spawn_worker_over, SpawnClient};
mod serve;
mod wire;
pub use wire::{RequestId, WorkerEnvelope, WorkerEvent, WorkerLine};

fn default_spawn_mouse() -> bool {
    true
}

/// Request to spawn a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub os_user: String,
    pub tool_path: String,
    /// Daemon's own resolved `tddy_data_dir` (see `ConnectionService::tddy_data_dir` /
    /// `DaemonConfig::tddy_data_dir`) — passed through verbatim so the worker process (forked
    /// before tokio starts, its own cwd unchanged) resolves it the same way `spawn_as_user`
    /// would in-process, never against `repo_path`.
    pub tddy_data_dir: String,
    pub repo_path: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub resume_session_id: Option<String>,
    /// New session with a fixed id (mutually exclusive with `resume_session_id` in the spawner).
    #[serde(default)]
    pub new_session_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Passed to spawned tddy-coder as --agent when non-empty.
    #[serde(default)]
    pub agent: Option<String>,
    /// The resolved `SpecializedAgentDef` for `agent` as JSON, passed to spawned tddy-coder as
    /// `--agent-def`. Set when the daemon resolved the name against its model registry, which the
    /// child cannot read. Omit in JSON for legacy clients.
    #[serde(default)]
    pub agent_def_json: Option<String>,
    /// The session's provider route as route-file JSON (`tddy_core::provider_route_file`), handed
    /// to the spawned `tddy-coder` as `--provider-route-path`. Omit in JSON for legacy clients.
    #[serde(default)]
    pub provider_route_json: Option<String>,
    /// Passed as `--mouse` when true (default). Omit in JSON for legacy clients.
    #[serde(default = "default_spawn_mouse")]
    pub mouse: bool,
    /// When set, spawned tool joins this LiveKit room (shared with browser presence). Omit for legacy clients.
    #[serde(default)]
    pub common_room: Option<String>,
    /// Multi-host: daemon instance id for LiveKit server identity. Omit for legacy clients.
    #[serde(default)]
    pub daemon_instance_id: Option<String>,
    /// Passed to spawned `tddy-coder` as `--recipe` when set (e.g. `bugfix`).
    #[serde(default)]
    pub recipe: Option<String>,
    /// Back-reference to the orchestrating PR-stack session. Passed as `--stack-parent <id>`.
    #[serde(default)]
    pub stack_parent: Option<String>,
    /// The planned node of that orchestrator's stack the session materializes. Passed as
    /// `--stack-node-id <id>`.
    #[serde(default)]
    pub stack_node_id: Option<String>,
    /// Existing session whose branch seeds a new `pr-stack` orchestrator's stack as its single root
    /// node. Passed as `--stack-seed-base-session <id>`.
    #[serde(default)]
    pub stack_seed_base_session: Option<String>,
    /// Passed to spawned `tddy-coder` as `--model` when set (tool-session model selection).
    #[serde(default)]
    pub model: Option<String>,
    /// Per-session unix socket path for the reverse `HostSessionService` (grill-me
    /// `spawn_conversation`). Passed to the child as `--host-session-socket`. A plain string, so it
    /// crosses this worker's JSON IPC unchanged — the reason the reverse channel is a socket path
    /// rather than the child's stdio fds (which can't cross the fork boundary).
    #[serde(default)]
    pub host_session_socket: Option<String>,
    /// The commit identity the child's agent authors under: the four `GIT_AUTHOR_*` /
    /// `GIT_COMMITTER_*` pairs of the account the session's project acts as. **Never a token** — the
    /// spawner refuses any other key, and a session's tools ask the daemon for the token per call
    /// over the toolcall socket. Absent on the wire means empty: this daemon and its worker (or the
    /// supervisor path) are separate processes that may be at different versions, and an old
    /// request must keep spawning under the checkout's own identity. Omit in JSON for legacy
    /// clients.
    #[serde(default)]
    pub git_environment: Vec<(String, String)>,
    /// `log.default.level` for the child's `--config` (from daemon YAML `log:`, e.g. `dev.desktop.yaml`).
    #[serde(default = "default_child_log_level")]
    pub child_log_level: String,
    /// `log.loggers.default.format` for the child session config.
    #[serde(default = "default_child_log_format")]
    pub child_log_format: String,
    /// Verbatim `log:` document (from the daemon's `coder_config_path`, e.g. `dev.config.yaml`) used
    /// as the child's `--config`. When set it owns the child's full log routing; when `None` the
    /// spawner synthesizes a minimal config from `child_log_level` / `child_log_format`.
    #[serde(default)]
    pub coder_log_config_yaml: Option<String>,
    /// How long the worker watches the spawned child for an immediate exit (daemon
    /// `spawn_startup_grace_period_ms`). It travels with the request because the worker is forked
    /// before the daemon's config is even env-overridden, so it cannot read it. Omit in JSON for
    /// legacy clients.
    #[serde(default = "default_startup_grace_period_ms")]
    pub startup_grace_period_ms: u64,
    /// How often that watch polls (daemon `spawn_startup_poll_interval_ms`). Omit in JSON for
    /// legacy clients.
    #[serde(default = "default_startup_poll_interval_ms")]
    pub startup_poll_interval_ms: u64,
}

fn default_startup_grace_period_ms() -> u64 {
    spawner::StartupWatch::default().as_millis().0
}

fn default_startup_poll_interval_ms() -> u64 {
    spawner::StartupWatch::default().as_millis().1
}

fn default_child_log_level() -> String {
    "debug".to_string()
}

fn default_child_log_format() -> String {
    spawner::CHILD_LOG_FORMAT_FALLBACK.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "payload")]
pub enum WorkerRequest {
    /// `SpawnRequest` is boxed to keep the enum variant small (clippy::large_enum_variant).
    #[serde(rename = "spawn")]
    Spawn(Box<SpawnRequest>),
    #[serde(rename = "clone")]
    Clone(CloneRequest),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum WorkerResponse {
    #[serde(rename = "spawn_ok")]
    SpawnOk { result: SpawnResult },
    #[serde(rename = "clone_ok")]
    CloneOk,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Fork the spawn worker before tokio starts. Returns (SpawnClient, worker_pid).
/// On non-Unix, returns None and spawns will use spawn_as_user directly (may deadlock).
#[cfg(unix)]
pub fn fork_spawn_worker() -> anyhow::Result<Option<(SpawnClient, libc::pid_t)>> {
    use std::os::unix::io::RawFd;

    let mut request_pipe: [RawFd; 2] = [0; 2];
    if unsafe { libc::pipe(request_pipe.as_mut_ptr()) } != 0 {
        anyhow::bail!("pipe() failed");
    }
    set_close_on_exec(request_pipe);
    let mut response_pipe: [RawFd; 2] = [0; 2];
    if unsafe { libc::pipe(response_pipe.as_mut_ptr()) } != 0 {
        unsafe {
            libc::close(request_pipe[0]);
            libc::close(request_pipe[1]);
        }
        anyhow::bail!("pipe() failed");
    }
    set_close_on_exec(response_pipe);

    let pid = unsafe { libc::fork() };
    match pid {
        -1 => {
            unsafe {
                libc::close(request_pipe[0]);
                libc::close(request_pipe[1]);
                libc::close(response_pipe[0]);
                libc::close(response_pipe[1]);
            }
            anyhow::bail!("fork() failed");
        }
        0 => {
            // Child: spawn worker
            unsafe {
                libc::close(request_pipe[1]);
                libc::close(response_pipe[0]);
            }
            let request_reader = unsafe { std::fs::File::from_raw_fd(request_pipe[0]) };
            let response_writer = unsafe { std::fs::File::from_raw_fd(response_pipe[1]) };
            serve::serve(request_reader, response_writer);
            std::process::exit(0);
        }
        _ => {
            // Parent
            unsafe {
                libc::close(request_pipe[0]);
                libc::close(response_pipe[1]);
            }
            let request_tx = unsafe { std::fs::File::from_raw_fd(request_pipe[1]) };
            let response_rx = unsafe { std::fs::File::from_raw_fd(response_pipe[0]) };
            let client = client_over(request_tx, response_rx, pid as u32);
            Ok(Some((client, pid)))
        }
    }
}

#[cfg(not(unix))]
pub fn fork_spawn_worker() -> anyhow::Result<Option<(SpawnClient, i32)>> {
    Ok(None)
}

/// Mark both ends of a pipe close-on-exec. A child the worker or the daemon execs then inherits
/// neither end, so the response pipe reaches EOF the moment the worker dies, however many children
/// it spawned and however long they run.
#[cfg(unix)]
fn set_close_on_exec(fds: [std::os::unix::io::RawFd; 2]) {
    for fd in fds {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
}

/// Run one spawn request on the worker's thread: `spawn_as_user` with the request's own parameters.
fn run_spawn(req: &SpawnRequest) -> WorkerResponse {
    let livekit = LiveKitCreds {
        url: req.livekit_url.clone(),
        api_key: req.livekit_api_key.clone(),
        api_secret: req.livekit_api_secret.clone(),
        common_room: req.common_room.clone(),
        daemon_instance_id: req.daemon_instance_id.clone(),
    };
    let session = req
        .resume_session_id
        .as_deref()
        .or(req.new_session_id.as_deref())
        .unwrap_or("new");
    log::info!(
        "spawn_worker: calling spawn_as_user session_id={} repo={}",
        session,
        req.repo_path
    );
    let result = spawner::spawn_as_user(
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
            // The reverse `spawn_conversation` channel is a per-session unix socket whose path
            // crosses this fork boundary as a plain string (unlike stdio fds), so it works from the
            // worker-spawned child just like the direct path.
            host_session_socket: req.host_session_socket.as_deref(),
            git_environment: &req.git_environment,
        },
        req.child_log_level.as_str(),
        req.child_log_format.as_str(),
        req.coder_log_config_yaml.as_deref(),
        spawner::StartupWatch::from_millis(
            req.startup_grace_period_ms,
            req.startup_poll_interval_ms,
        ),
    );
    log::info!(
        "spawn_worker: spawn_as_user returned session_id={}",
        session
    );
    match result {
        Ok(result) => WorkerResponse::SpawnOk { result },
        Err(e) => {
            log::warn!(
                "spawn_worker: spawn_as_user err session_id={} err={}",
                session,
                e
            );
            WorkerResponse::Error {
                message: e.to_string(),
            }
        }
    }
}

/// Build spawn request from connection service args.
#[allow(clippy::too_many_arguments)] // spawn-time argument bundle mirroring spawn_as_user; a struct would obscure call sites.
pub fn build_spawn_request(
    os_user: &str,
    tool_path: &str,
    tddy_data_dir: &Path,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    opts: SpawnOptions<'_>,
    daemon_log: Option<&tddy_core::LogConfig>,
    coder_log_config_yaml: Option<String>,
    startup: spawner::StartupWatch,
) -> SpawnRequest {
    let (child_log_level, child_log_format) = spawner::child_log_yaml_tuning(daemon_log);
    let (startup_grace_period_ms, startup_poll_interval_ms) = startup.as_millis();
    SpawnRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_string(),
        tddy_data_dir: tddy_data_dir.display().to_string(),
        repo_path: repo_path.display().to_string(),
        livekit_url: livekit.url.clone(),
        livekit_api_key: livekit.api_key.clone(),
        livekit_api_secret: livekit.api_secret.clone(),
        resume_session_id: opts.resume_session_id.map(String::from),
        new_session_id: opts.new_session_id.map(String::from),
        project_id: opts.project_id.map(String::from),
        agent: opts.agent.map(String::from),
        agent_def_json: opts.agent_def_json.map(String::from),
        provider_route_json: opts.provider_route_json.map(String::from),
        mouse: opts.mouse,
        common_room: livekit.common_room.clone(),
        daemon_instance_id: livekit.daemon_instance_id.clone(),
        recipe: opts.recipe.map(String::from),
        stack_parent: opts.stack_parent.map(String::from),
        stack_node_id: opts.stack_node_id.map(String::from),
        stack_seed_base_session: opts.stack_seed_base_session.map(String::from),
        model: opts.model.map(String::from),
        host_session_socket: opts.host_session_socket.map(String::from),
        git_environment: opts.git_environment.to_vec(),
        child_log_level,
        child_log_format,
        coder_log_config_yaml,
        startup_grace_period_ms,
        startup_poll_interval_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    fn a_livekit() -> LiveKitCreds {
        LiveKitCreds {
            url: "ws://localhost:7880".to_string(),
            api_key: "key".to_string(),
            api_secret: "secret".to_string(),
            common_room: None,
            daemon_instance_id: None,
        }
    }

    fn a_spawn_request_with(
        opts: SpawnOptions<'_>,
        startup: spawner::StartupWatch,
    ) -> SpawnRequest {
        build_spawn_request(
            "dev",
            "/usr/bin/tddy-coder",
            Path::new("/data"),
            Path::new("/repo"),
            &a_livekit(),
            opts,
            None,
            None,
            startup,
        )
    }

    #[test]
    fn build_spawn_request_carries_the_model_through_to_the_spawn_request() {
        // Given — a tool spawn whose selected model is opus
        let opts = SpawnOptions {
            model: Some("opus"),
            ..Default::default()
        };

        // When
        let req = a_spawn_request_with(opts, spawner::StartupWatch::default());

        // Then — the model rides along so the worker can pass `--model opus`
        assert_eq!(req.model.as_deref(), Some("opus"));
    }

    #[test]
    fn build_spawn_request_carries_the_configured_startup_watch_across_the_fork() {
        // Given a daemon whose configured startup watch is not the default one
        let startup = spawner::StartupWatch {
            grace: Duration::from_millis(4000),
            poll: Duration::from_millis(200),
        };

        // When building the request the forked worker will act on
        let req = a_spawn_request_with(SpawnOptions::default(), startup);

        // Then the worker can reconstruct that same watch — it cannot read the daemon's config
        assert_eq!(req.startup_grace_period_ms, 4000);
        assert_eq!(req.startup_poll_interval_ms, 200);
        assert_eq!(
            spawner::StartupWatch::from_millis(
                req.startup_grace_period_ms,
                req.startup_poll_interval_ms
            ),
            startup
        );
    }

    #[test]
    fn a_spawn_request_from_a_legacy_client_watches_for_the_production_default() {
        // Given a request JSON written before the startup watch was configurable
        let json = r#"{
            "os_user": "dev",
            "tool_path": "/usr/bin/tddy-coder",
            "tddy_data_dir": "/data",
            "repo_path": "/repo",
            "livekit_url": "ws://localhost:7880",
            "livekit_api_key": "key",
            "livekit_api_secret": "secret",
            "resume_session_id": null
        }"#;

        // When the worker decodes it
        let req: SpawnRequest = serde_json::from_str(json).expect("decode legacy spawn request");

        // Then it watches for exactly as long as it always did
        assert_eq!(
            spawner::StartupWatch::from_millis(
                req.startup_grace_period_ms,
                req.startup_poll_interval_ms
            ),
            spawner::StartupWatch::default()
        );
    }

    fn the_commit_identity_of_ada() -> Vec<(String, String)> {
        [
            ("GIT_AUTHOR_NAME", "ada"),
            ("GIT_AUTHOR_EMAIL", "101+ada@users.noreply.github.com"),
            ("GIT_COMMITTER_NAME", "ada"),
            ("GIT_COMMITTER_EMAIL", "101+ada@users.noreply.github.com"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .to_vec()
    }

    #[test]
    fn build_spawn_request_carries_the_commit_identity_through_to_the_spawn_request() {
        // Given a tool spawn for a project that acts as ada
        let identity = the_commit_identity_of_ada();
        let opts = SpawnOptions {
            git_environment: &identity,
            ..Default::default()
        };

        // When
        let req = a_spawn_request_with(opts, spawner::StartupWatch::default());

        // Then the worker is handed her four pairs
        assert_eq!(req.git_environment, identity);
    }

    #[test]
    fn a_spawn_request_round_trips_its_commit_identity_over_the_worker_pipe() {
        // Given a request carrying ada's identity
        let identity = the_commit_identity_of_ada();
        let sent = a_spawn_request_with(
            SpawnOptions {
                git_environment: &identity,
                ..Default::default()
            },
            spawner::StartupWatch::default(),
        );

        // When it crosses the worker's JSON IPC
        let wire = serde_json::to_string(&WorkerRequest::Spawn(Box::new(sent))).unwrap();
        let received = match serde_json::from_str::<WorkerRequest>(&wire).unwrap() {
            WorkerRequest::Spawn(req) => req,
            WorkerRequest::Clone(_) => panic!("a spawn request decoded as a clone"),
        };

        // Then the same pairs arrive
        assert_eq!(received.git_environment, identity);
    }

    #[test]
    fn a_spawn_request_from_a_client_that_predates_the_commit_identity_carries_none() {
        // Given a request written by a daemon that had no such field
        let mut json = serde_json::to_value(a_spawn_request_with(
            SpawnOptions::default(),
            spawner::StartupWatch::default(),
        ))
        .unwrap();
        json.as_object_mut()
            .expect("a request is a JSON object")
            .remove("git_environment");

        // When the worker decodes it
        let req: SpawnRequest = serde_json::from_value(json).expect("decode old spawn request");

        // Then no identity is added — the checkout's own stays in force
        assert_eq!(req.git_environment, Vec::<(String, String)>::new());
    }
}
