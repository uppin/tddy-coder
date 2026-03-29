//! Per-connection sandbox host: exec, VFS objects, checksum helper, and rsync TCP bridge.
//!
//! **Security (PRD N2):** Connect-RPC currently attaches empty [`tddy_rpc::RequestMetadata`] (see
//! `tddy-connectrpc`); LiveKit may populate `sender_identity`. Full bearer/cookie validation for
//! RemoteSandbox should be wired when the transport forwards credentials into metadata — see
//! `RequestMetadata` and the Connect router.

use std::collections::HashMap;
use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::sync::Mutex;

use tddy_rpc::{Code, Request, Response, Status};

use crate::proto::remote_sandbox_v1::{
    ExecChecksumRequest, ExecChecksumResponse, ExecNonInteractiveRequest,
    ExecNonInteractiveResponse, OpenRsyncSessionRequest, OpenRsyncSessionResponse,
    PutObjectRequest, PutObjectResponse, RemoteSandboxService, StatObjectRequest,
    StatObjectResponse,
};
use crate::sandbox_path::sandbox_relative_path;

/// Maximum captured stdout for [`RemoteSandboxServiceImpl::exec_non_interactive`] (resource limit, PRD N3).
const MAX_EXEC_NONINTERACTIVE_STDOUT: usize = 16 * 1024 * 1024;

/// Shared sandbox roots keyed by logical session id (isolates concurrent Connect / rsync clients).
#[derive(Debug)]
pub struct SandboxRegistry {
    inner: Mutex<HashMap<String, PathBuf>>,
}

impl SandboxRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the on-disk root for `session`, creating a fresh temp directory on first use.
    pub async fn root_for_session(&self, session: &str) -> Result<PathBuf, Status> {
        let mut g = self.inner.lock().await;
        if let Some(p) = g.get(session) {
            return Ok(p.clone());
        }
        let p = std::env::temp_dir()
            .join("tddy-remote-sandbox")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&p).map_err(|e| {
            Status::internal(format!("create session sandbox root {}: {e}", p.display()))
        })?;
        log::info!(
            "remote sandbox: new session root session_id={} path={}",
            session,
            p.display()
        );
        g.insert(session.to_string(), p.clone());
        Ok(p)
    }
}

impl Default for SandboxRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Daemon-side `RemoteSandboxService` implementation.
#[derive(Clone)]
pub struct RemoteSandboxServiceImpl {
    registry: Arc<SandboxRegistry>,
}

impl Default for RemoteSandboxServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteSandboxServiceImpl {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(SandboxRegistry::new()),
        }
    }
}

fn session_key(raw: &str) -> &str {
    if raw.is_empty() {
        "default"
    } else {
        raw
    }
}

fn map_path_err(e: &'static str) -> Status {
    Status::invalid_argument(e)
}

#[cfg(unix)]
fn run_rsync_server_bridge_blocking(
    tcp: TcpStream,
    root: PathBuf,
    args: Vec<String>,
) -> Result<(), String> {
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    tcp.set_nonblocking(false)
        .map_err(|e| format!("rsync tcp set_blocking: {e}"))?;

    let fd = tcp.into_raw_fd();
    let in_fd = unsafe { libc::dup(fd) };
    let out_fd = unsafe { libc::dup(fd) };
    unsafe {
        libc::close(fd);
    }
    if in_fd < 0 || out_fd < 0 {
        unsafe {
            if in_fd >= 0 {
                libc::close(in_fd);
            }
            if out_fd >= 0 {
                libc::close(out_fd);
            }
        }
        return Err(format!(
            "rsync bridge: dup(2) failed: {}",
            io::Error::last_os_error()
        ));
    }

    log::info!(
        "rsync server: spawning rsync --server with stdin/stdout on duplicated socket fds (in={in_fd} out={out_fd})"
    );

    let mut cmd = std::process::Command::new("rsync");
    cmd.args(&args);
    cmd.current_dir(&root);
    unsafe {
        cmd.stdin(Stdio::from_raw_fd(in_fd));
        cmd.stdout(Stdio::from_raw_fd(out_fd));
    }
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("rsync spawn: {e}"))?;
    let status = child.wait().map_err(|e| format!("rsync wait: {e}"))?;
    log::debug!("rsync --server child finished: {status}");
    Ok(())
}

#[cfg(not(unix))]
fn run_rsync_server_bridge_blocking(
    _tcp: TcpStream,
    _root: PathBuf,
    _args: Vec<String>,
) -> Result<(), String> {
    Err("OpenRsyncSession rsync bridge is only supported on Unix".to_string())
}

/// Without `--mkpath`, rsync's remote receiver calls `mkdir` on the full destination path and fails
/// if parent directories are missing (`get_local_name` → `do_mkdir`). Match OpenSSH convenience by
/// injecting `--mkpath` after `--server` when the client did not pass it.
fn inject_rsync_server_mkpath(args: &mut Vec<String>) {
    const MKPATH: &str = "--mkpath";
    if args.iter().any(|a| a == MKPATH) {
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--server") {
        args.insert(i + 1, MKPATH.to_string());
        log::info!("OpenRsyncSession: injected {MKPATH} for nested sandbox destination paths");
    }
}

#[async_trait]
impl RemoteSandboxService for RemoteSandboxServiceImpl {
    async fn exec_non_interactive(
        &self,
        request: Request<ExecNonInteractiveRequest>,
    ) -> Result<Response<ExecNonInteractiveResponse>, Status> {
        let r = request.into_inner();
        log::debug!("ExecNonInteractive argv_json_len={}", r.argv_json.len());
        let argv: Vec<String> = if r.argv_json.trim().is_empty() {
            vec![]
        } else {
            serde_json::from_str(&r.argv_json).map_err(|e| {
                Status::invalid_argument(format!("argv_json is not a JSON string array: {e}"))
            })?
        };
        if argv.is_empty() {
            log::info!("ExecNonInteractive empty argv — treating as no-op success");
            return Ok(Response::new(ExecNonInteractiveResponse {
                exit_code: 0,
                stdout: Vec::new(),
            }));
        }
        let session_key = session_key(&r.session);
        let root = self.registry.root_for_session(session_key).await?;
        log::info!(
            "ExecNonInteractive session={} program={} argc={} cwd={}",
            session_key,
            argv[0],
            argv.len(),
            root.display()
        );
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.current_dir(&root);
        cmd.kill_on_drop(true);
        let out = cmd
            .output()
            .await
            .map_err(|e| Status::internal(format!("spawn/exec failed: {e}")))?;
        let code = out.status.code().unwrap_or(-1);
        if out.stdout.len() > MAX_EXEC_NONINTERACTIVE_STDOUT {
            return Err(Status {
                code: Code::ResourceExhausted,
                message: format!(
                    "stdout exceeds limit of {} bytes",
                    MAX_EXEC_NONINTERACTIVE_STDOUT
                ),
            });
        }
        log::debug!(
            "ExecNonInteractive exited code={} stdout_len={}",
            code,
            out.stdout.len()
        );
        Ok(Response::new(ExecNonInteractiveResponse {
            exit_code: code,
            stdout: out.stdout,
        }))
    }

    async fn put_object(
        &self,
        request: Request<PutObjectRequest>,
    ) -> Result<Response<PutObjectResponse>, Status> {
        let r = request.into_inner();
        let rel = sandbox_relative_path(&r.path).map_err(map_path_err)?;
        let root = self
            .registry
            .root_for_session(session_key(&r.session))
            .await?;
        let full = root.join(&rel);
        log::info!(
            "PutObject session={} rel={} bytes={}",
            r.session,
            rel.display(),
            r.content.len()
        );
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Status::internal(format!("mkdir parents: {e}")))?;
        }
        tokio::fs::write(&full, &r.content)
            .await
            .map_err(|e| Status::internal(format!("write object: {e}")))?;
        Ok(Response::new(PutObjectResponse {}))
    }

    async fn stat_object(
        &self,
        request: Request<StatObjectRequest>,
    ) -> Result<Response<StatObjectResponse>, Status> {
        let r = request.into_inner();
        let rel = sandbox_relative_path(&r.path).map_err(map_path_err)?;
        let root = self
            .registry
            .root_for_session(session_key(&r.session))
            .await?;
        let full = root.join(&rel);
        log::debug!("StatObject session={} path={}", r.session, full.display());
        match tokio::fs::metadata(&full).await {
            Ok(m) if m.is_file() => Ok(Response::new(StatObjectResponse { size: m.len() })),
            Ok(_) => Err(Status::failed_precondition("not a regular file")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(Status::not_found("object not found"))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn exec_checksum(
        &self,
        request: Request<ExecChecksumRequest>,
    ) -> Result<Response<ExecChecksumResponse>, Status> {
        let _ = request;
        let root = self.registry.root_for_session("default").await?;
        log::info!("ExecChecksum (fixed smoke payload) cwd={}", root.display());
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg("printf '%s' 'livekit-exec-checksum-smoke'");
        cmd.current_dir(&root);
        cmd.kill_on_drop(true);
        let out = cmd
            .output()
            .await
            .map_err(|e| Status::internal(format!("exec_checksum spawn: {e}")))?;
        let code = out.status.code().unwrap_or(-1);
        let digest = Sha256::digest(&out.stdout);
        log::debug!(
            "ExecChecksum exit={} stdout_len={} sha256_len={}",
            code,
            out.stdout.len(),
            digest.len()
        );
        Ok(Response::new(ExecChecksumResponse {
            stdout_sha256: digest.to_vec(),
            exit_code: code,
        }))
    }

    async fn open_rsync_session(
        &self,
        request: Request<OpenRsyncSessionRequest>,
    ) -> Result<Response<OpenRsyncSessionResponse>, Status> {
        let r = request.into_inner();
        let argv: Vec<String> = serde_json::from_str(&r.argv_json).map_err(|e| {
            Status::invalid_argument(format!("argv_json is not a JSON string array: {e}"))
        })?;
        if argv.is_empty() {
            return Err(Status::invalid_argument("OpenRsyncSession argv is empty"));
        }
        let argv0 = std::path::Path::new(&argv[0])
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(argv[0].as_str());
        if argv0 != "rsync" {
            return Err(Status::invalid_argument(format!(
                "OpenRsyncSession argv[0] must be rsync, got {argv0:?}"
            )));
        }
        let root = self
            .registry
            .root_for_session(session_key(&r.session))
            .await?;
        let mut args: Vec<String> = argv[1..].to_vec();
        inject_rsync_server_mkpath(&mut args);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| Status::internal(format!("bind rsync tcp: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| Status::internal(e.to_string()))?
            .port();
        log::info!(
            "OpenRsyncSession session={} port={} argc={} root={}",
            r.session,
            port,
            args.len(),
            root.display()
        );

        tokio::spawn(async move {
            let accept = listener.accept().await;
            let Ok((socket, peer)) = accept else {
                log::warn!("OpenRsyncSession accept failed or listener closed");
                return;
            };
            log::debug!("OpenRsyncSession peer={peer} spawning rsync --server");
            let std_sock = match socket.into_std() {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("OpenRsyncSession into_std: {e}");
                    return;
                }
            };
            match tokio::task::spawn_blocking(move || {
                run_rsync_server_bridge_blocking(std_sock, root, args)
            })
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::warn!("OpenRsyncSession rsync bridge error: {e}"),
                Err(e) => log::warn!("OpenRsyncSession join bridge task: {e}"),
            }
        });

        Ok(Response::new(OpenRsyncSessionResponse {
            host: "127.0.0.1".to_string(),
            port: port as u32,
        }))
    }
}
