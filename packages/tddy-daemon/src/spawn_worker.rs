//! Spawn worker — single-threaded child process that performs fork+exec.
//!
//! Fork from a multi-threaded tokio process can deadlock (pthread/malloc locks).
//! We fork this worker before tokio starts, so it has only one thread and can
//! safely spawn tddy-* processes.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::spawner::{self, LiveKitCreds, SpawnResult};

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
}

/// Response from spawn worker.
#[derive(Debug, Serialize, Deserialize)]
pub enum SpawnResponse {
    Ok(SpawnResult),
    Err(String),
}

/// Client for sending spawn requests to the worker. Used by the async daemon.
pub struct SpawnClient {
    request_tx: Arc<std::sync::Mutex<std::fs::File>>,
    response_rx: Arc<std::sync::Mutex<BufReader<std::fs::File>>>,
}

impl SpawnClient {
    pub fn spawn(&self, req: SpawnRequest) -> anyhow::Result<SpawnResult> {
        let session_id = req.resume_session_id.as_deref().unwrap_or("new");
        log::debug!("SpawnClient: sending request session_id={}", session_id);
        let request_json = serde_json::to_string(&req)?;
        let mut tx = self.request_tx.lock().unwrap();
        writeln!(tx, "{}", request_json)?;
        tx.flush()?;
        drop(tx);

        log::debug!(
            "SpawnClient: waiting for response session_id={}",
            session_id
        );
        let mut rx = self.response_rx.lock().unwrap();
        let mut line = String::new();
        rx.read_line(&mut line)?;
        log::debug!("SpawnClient: got response session_id={}", session_id);
        let response: SpawnResponse = serde_json::from_str(line.trim())?;
        match response {
            SpawnResponse::Ok(r) => Ok(r),
            SpawnResponse::Err(e) => Err(anyhow::anyhow!("{}", e)),
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
        let req: SpawnRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(
                    response_writer,
                    "{}",
                    serde_json::to_string(&SpawnResponse::Err(e.to_string())).unwrap()
                );
                let _ = response_writer.flush();
                continue;
            }
        };

        let livekit = LiveKitCreds {
            url: req.livekit_url.clone(),
            api_key: req.livekit_api_key.clone(),
            api_secret: req.livekit_api_secret.clone(),
        };
        log::info!(
            "spawn_worker: calling spawn_as_user session_id={}",
            req.resume_session_id.as_deref().unwrap_or("new")
        );
        let result = spawner::spawn_as_user(
            &req.os_user,
            &req.tool_path,
            Path::new(&req.repo_path),
            &livekit,
            req.resume_session_id.as_deref(),
        );
        log::info!(
            "spawn_worker: spawn_as_user returned session_id={}",
            req.resume_session_id.as_deref().unwrap_or("new")
        );

        let response = match result {
            Ok(r) => SpawnResponse::Ok(r),
            Err(e) => SpawnResponse::Err(e.to_string()),
        };
        let response_json = serde_json::to_string(&response).unwrap();
        let _ = writeln!(response_writer, "{}", response_json);
        let _ = response_writer.flush();
    }
}

/// Build spawn request from connection service args.
pub fn build_spawn_request(
    os_user: &str,
    tool_path: &str,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    resume_session_id: Option<&str>,
) -> SpawnRequest {
    SpawnRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_string(),
        repo_path: repo_path.display().to_string(),
        livekit_url: livekit.url.clone(),
        livekit_api_key: livekit.api_key.clone(),
        livekit_api_secret: livekit.api_secret.clone(),
        resume_session_id: resume_session_id.map(String::from),
    }
}
