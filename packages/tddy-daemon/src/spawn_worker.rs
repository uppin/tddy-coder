//! Spawn worker — single-threaded child process that performs fork+exec.
//!
//! Fork from a multi-threaded tokio process can deadlock (pthread/malloc locks).
//! We fork this worker before tokio starts, so it has only one thread and can
//! safely spawn tddy-* processes.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::spawner::{self, LiveKitCreds, SpawnOptions, SpawnResult};

fn default_spawn_mouse() -> bool {
    true
}

/// Request to spawn a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub os_user: String,
    pub tool_path: String,
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
    /// `log.default.level` for the child's `--config` (from daemon YAML `log:`, e.g. `dev.desktop.yaml`).
    #[serde(default = "default_child_log_level")]
    pub child_log_level: String,
    /// `log.loggers.default.format` for the child session config.
    #[serde(default = "default_child_log_format")]
    pub child_log_format: String,
}

fn default_child_log_level() -> String {
    "debug".to_string()
}

fn default_child_log_format() -> String {
    spawner::CHILD_LOG_FORMAT_FALLBACK.to_string()
}

/// Request to clone a git repository as an OS user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneRequest {
    pub os_user: String,
    pub git_url: String,
    pub destination: String,
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

/// Client for sending spawn requests to the worker. Used by the async daemon.
#[derive(Clone)]
pub struct SpawnClient {
    request_tx: Arc<std::sync::Mutex<std::fs::File>>,
    response_rx: Arc<std::sync::Mutex<BufReader<std::fs::File>>>,
}

fn worker_request_label(req: &WorkerRequest) -> String {
    match req {
        WorkerRequest::Spawn(s) => format!(
            "op=spawn session={} repo={}",
            s.resume_session_id
                .as_deref()
                .or(s.new_session_id.as_deref())
                .unwrap_or("new"),
            s.repo_path
        ),
        WorkerRequest::Clone(c) => format!("op=clone dest={} os_user={}", c.destination, c.os_user),
    }
}

impl SpawnClient {
    fn send_and_recv(&self, req: WorkerRequest) -> anyhow::Result<WorkerResponse> {
        let label = worker_request_label(&req);
        let started = Instant::now();
        log::info!("SpawnClient: sending {}", label);
        let request_json = serde_json::to_string(&req)?;
        let mut tx = self.request_tx.lock().unwrap();
        writeln!(tx, "{}", request_json)?;
        tx.flush()?;
        drop(tx);

        log::info!("SpawnClient: waiting for worker {}", label);
        let mut rx = self.response_rx.lock().unwrap();
        let mut line = String::new();
        let n = rx.read_line(&mut line)?;
        if n == 0 && line.is_empty() {
            anyhow::bail!(
                "spawn worker closed response pipe without a line (EOF) after {:?}",
                started.elapsed()
            );
        }
        let elapsed = started.elapsed();
        log::info!(
            "SpawnClient: received response line ({} bytes) elapsed_ms={} {}",
            line.len(),
            elapsed.as_millis(),
            label
        );
        let response: WorkerResponse = serde_json::from_str(line.trim()).map_err(|e| {
            let preview: String = line.chars().take(256).collect();
            anyhow::anyhow!(
                "invalid WorkerResponse JSON from spawn worker: {} (preview: {:?})",
                e,
                preview
            )
        })?;
        Ok(response)
    }

    pub fn spawn(&self, req: SpawnRequest) -> anyhow::Result<SpawnResult> {
        match self.send_and_recv(WorkerRequest::Spawn(Box::new(req)))? {
            WorkerResponse::SpawnOk { result } => Ok(result),
            WorkerResponse::Error { message } => Err(anyhow::anyhow!("{}", message)),
            WorkerResponse::CloneOk => Err(anyhow::anyhow!("unexpected clone_ok for spawn")),
        }
    }

    pub fn clone_repo(&self, req: CloneRequest) -> anyhow::Result<()> {
        match self.send_and_recv(WorkerRequest::Clone(req))? {
            WorkerResponse::CloneOk => Ok(()),
            WorkerResponse::Error { message } => Err(anyhow::anyhow!("{}", message)),
            WorkerResponse::SpawnOk { .. } => Err(anyhow::anyhow!("unexpected spawn_ok for clone")),
        }
    }
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
    let mut response_pipe: [RawFd; 2] = [0; 2];
    if unsafe { libc::pipe(response_pipe.as_mut_ptr()) } != 0 {
        unsafe {
            libc::close(request_pipe[0]);
            libc::close(request_pipe[1]);
        }
        anyhow::bail!("pipe() failed");
    }

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
            spawn_worker_main(request_pipe[0], response_pipe[1]);
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
            let client = SpawnClient {
                request_tx: Arc::new(std::sync::Mutex::new(request_tx)),
                response_rx: Arc::new(std::sync::Mutex::new(BufReader::new(response_rx))),
            };
            Ok(Some((client, pid)))
        }
    }
}

#[cfg(not(unix))]
pub fn fork_spawn_worker() -> anyhow::Result<Option<(SpawnClient, i32)>> {
    Ok(None)
}

/// Worker main loop: read requests, spawn, write responses.
#[cfg(unix)]
fn spawn_worker_main(request_fd: libc::c_int, response_fd: libc::c_int) {
    let request_reader = unsafe { std::fs::File::from_raw_fd(request_fd) };
    let mut response_writer = unsafe { std::fs::File::from_raw_fd(response_fd) };

    log::info!("spawn_worker: started, waiting for requests");
    let reader = BufReader::new(request_reader);
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        log::info!("spawn_worker: received request ({} bytes)", line.len());

        let req: WorkerRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(
                    response_writer,
                    "{}",
                    serde_json::to_string(&WorkerResponse::Error {
                        message: e.to_string()
                    })
                    .unwrap()
                );
                let _ = response_writer.flush();
                continue;
            }
        };

        let response = match req {
            WorkerRequest::Spawn(req) => {
                let livekit = LiveKitCreds {
                    url: req.livekit_url.clone(),
                    api_key: req.livekit_api_key.clone(),
                    api_secret: req.livekit_api_secret.clone(),
                    common_room: req.common_room.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                };
                log::info!(
                    "spawn_worker: calling spawn_as_user session_id={} repo={}",
                    req.resume_session_id
                        .as_deref()
                        .or(req.new_session_id.as_deref())
                        .unwrap_or("new"),
                    req.repo_path
                );
                let result = spawner::spawn_as_user(
                    &req.os_user,
                    &req.tool_path,
                    Path::new(&req.repo_path),
                    &livekit,
                    SpawnOptions {
                        resume_session_id: req.resume_session_id.as_deref(),
                        new_session_id: req.new_session_id.as_deref(),
                        project_id: req.project_id.as_deref(),
                        agent: req.agent.as_deref(),
                        mouse: req.mouse,
                        recipe: req.recipe.as_deref(),
                    },
                    req.child_log_level.as_str(),
                    req.child_log_format.as_str(),
                );
                log::info!(
                    "spawn_worker: spawn_as_user returned session_id={}",
                    req.resume_session_id
                        .as_deref()
                        .or(req.new_session_id.as_deref())
                        .unwrap_or("new")
                );
                match &result {
                    Ok(_) => {}
                    Err(e) => log::warn!(
                        "spawn_worker: spawn_as_user err session_id={} err={}",
                        req.resume_session_id
                            .as_deref()
                            .or(req.new_session_id.as_deref())
                            .unwrap_or("new"),
                        e
                    ),
                }
                match result {
                    Ok(r) => WorkerResponse::SpawnOk { result: r },
                    Err(e) => WorkerResponse::Error {
                        message: e.to_string(),
                    },
                }
            }
            WorkerRequest::Clone(req) => {
                let dest = Path::new(&req.destination);
                log::info!(
                    "spawn_worker: calling clone_as_user os_user={} dest={}",
                    req.os_user,
                    req.destination
                );
                let result = spawner::clone_as_user(&req.os_user, &req.git_url, dest);
                match &result {
                    Ok(()) => log::info!("spawn_worker: clone_as_user ok dest={}", req.destination),
                    Err(e) => log::warn!(
                        "spawn_worker: clone_as_user err dest={} err={}",
                        req.destination,
                        e
                    ),
                }
                match result {
                    Ok(()) => WorkerResponse::CloneOk,
                    Err(e) => WorkerResponse::Error {
                        message: e.to_string(),
                    },
                }
            }
        };

        let response_json = serde_json::to_string(&response).unwrap();
        if let Err(e) = writeln!(response_writer, "{}", response_json) {
            log::error!("spawn_worker: failed to write response line: {}", e);
        }
        if let Err(e) = response_writer.flush() {
            log::error!("spawn_worker: failed to flush response pipe: {}", e);
        }
    }
}

/// Build spawn request from connection service args.
pub fn build_spawn_request(
    os_user: &str,
    tool_path: &str,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    opts: SpawnOptions<'_>,
    daemon_log: Option<&tddy_core::LogConfig>,
) -> SpawnRequest {
    let (child_log_level, child_log_format) = spawner::child_log_yaml_tuning(daemon_log);
    SpawnRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_string(),
        repo_path: repo_path.display().to_string(),
        livekit_url: livekit.url.clone(),
        livekit_api_key: livekit.api_key.clone(),
        livekit_api_secret: livekit.api_secret.clone(),
        resume_session_id: opts.resume_session_id.map(String::from),
        new_session_id: opts.new_session_id.map(String::from),
        project_id: opts.project_id.map(String::from),
        agent: opts.agent.map(String::from),
        mouse: opts.mouse,
        common_room: livekit.common_room.clone(),
        daemon_instance_id: livekit.daemon_instance_id.clone(),
        recipe: opts.recipe.map(String::from),
        child_log_level,
        child_log_format,
    }
}
