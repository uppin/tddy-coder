//! Screen sharing control-plane service for tddy-daemon.
//!
//! Implements `ScreenSharingService` from `screen_sharing.proto` over the HTTP Connect
//! transport. Manages the encrypted vault per session, caches derived keys in memory,
//! and spawns/terminates protocol bridge processes (tddy-vnc / tddy-rdp) on demand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use log::{error, info};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::{resolve_rdp_binary_path, resolve_vnc_binary_path, DaemonConfig};
use crate::screen_sharing_vault::{
    vault_path, DerivedKey, ScreenSharingTarget, ScreenSharingVault,
};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    AddTargetRequest, AddTargetResponse, ListTargetsRequest, ListTargetsResponse, Protocol,
    RemoveTargetRequest, RemoveTargetResponse, ScreenSharingService,
    ScreenSharingTarget as ProtoScreenSharingTarget, StartStreamRequest, StartStreamResponse,
    StopStreamRequest, StopStreamResponse, UnlockVaultRequest, UnlockVaultResponse,
};

const DEFAULT_STREAM_WIDTH: u32 = 1920;
const DEFAULT_STREAM_HEIGHT: u32 = 1080;
const DEFAULT_STREAM_FPS: u32 = 30;

/// Per-session derived key cache: session_id → DerivedKey.
///
/// Populated by `UnlockVault`; read by `AddTarget` and `StartStream`.
pub type ScreenSharingKeyCache = Arc<Mutex<HashMap<String, DerivedKey>>>;

type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
/// Per-OS-user sessions base resolver: `Arc<dyn Fn(&str) -> Option<PathBuf>>`.
///
/// Wired in `main.rs` with the daemon's resolved `tddy_data_dir` (config-only tddy home) so
/// screen-sharing vaults live under the same data root as session trees, not a static `$HOME/.tddy`.
pub type SessionsBase = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Bridge config serialized to the bridge binary's stdin (field names match `tddy_screenshare::BridgeConfig`).
#[derive(serde::Serialize)]
struct BridgeSpawnConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
    livekit_url: String,
    livekit_token: String,
    livekit_room: String,
    livekit_identity: String,
    track_name: String,
    width: u32,
    height: u32,
    target_id: String,
    fps: u32,
}

/// Daemon-side implementation of `ScreenSharingService`.
pub struct ScreenSharingServiceImpl {
    user_resolver: UserResolver,
    sessions_base: SessionsBase,
    key_cache: ScreenSharingKeyCache,
    /// Optional daemon config — when set, `start_stream` spawns real bridge processes.
    config: Option<Arc<DaemonConfig>>,
    /// Active bridge PIDs keyed by `"{session_id}:{target_id}"` for `stop_stream`.
    active_bridges: Arc<Mutex<HashMap<String, u32>>>,
}

impl ScreenSharingServiceImpl {
    pub fn new(
        user_resolver: UserResolver,
        sessions_base: SessionsBase,
        key_cache: ScreenSharingKeyCache,
    ) -> Self {
        Self {
            user_resolver,
            sessions_base,
            key_cache,
            config: None,
            active_bridges: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Supply daemon config for bridge binary path resolution and LiveKit token generation.
    pub fn with_config(mut self, config: Arc<DaemonConfig>) -> Self {
        self.config = Some(config);
        self
    }

    fn resolve_session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        let user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid session token"))?;
        let base = (self.sessions_base)(&user)
            .ok_or_else(|| Status::internal("sessions base not found for user"))?;
        Ok(base.join("sessions").join(session_id))
    }

    async fn require_key(&self, session_id: &str) -> Result<DerivedKey, Status> {
        let cache = self.key_cache.lock().await;
        cache
            .get(session_id)
            .cloned()
            .ok_or_else(|| Status::failed_precondition("vault not unlocked"))
    }

    /// Attempt to spawn a bridge process for the given target.
    ///
    /// Logs all errors; never returns an error — the caller returns pre-computed LiveKit
    /// coordinates regardless of whether the bridge process spawns successfully.
    #[allow(clippy::too_many_arguments)]
    async fn try_spawn_bridge(
        &self,
        config: &DaemonConfig,
        target: &ScreenSharingTarget,
        username: String,
        password: String,
        bridge_identity: &str,
        track_name: &str,
        livekit_room: &str,
        session_id: &str,
    ) {
        let spawn_config = match build_bridge_spawn_config(
            config,
            target,
            username,
            password,
            bridge_identity,
            track_name,
            livekit_room,
        ) {
            Some(c) => c,
            None => return, // already logged
        };

        let config_json = match serde_json::to_vec(&spawn_config) {
            Ok(j) => j,
            Err(e) => {
                error!("bridge spawn skipped: failed to serialize config: {}", e);
                return;
            }
        };

        let binary = match target.protocol {
            Protocol::Vnc => resolve_vnc_binary_path(config),
            Protocol::Rdp => resolve_rdp_binary_path(config),
            Protocol::Unspecified => {
                error!(
                    "bridge spawn skipped: unspecified protocol for target {}",
                    target.id
                );
                return;
            }
        };

        let mut child = match Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                error!("failed to spawn bridge binary '{}': {}", binary, e);
                return;
            }
        };

        // Write the JSON config to the bridge's stdin, then close it.
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(&config_json).await {
                error!("failed to write config to bridge stdin: {}", e);
                let _ = child.kill().await;
                return;
            }
            // stdin dropped here → pipe closed → bridge reads EOF and proceeds
        }

        let bridge_key = format!("{}:{}", session_id, target.id);

        // Store the PID so stop_stream can send SIGTERM.
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            self.active_bridges
                .lock()
                .await
                .insert(bridge_key.clone(), pid);
        }

        info!(
            "bridge spawned: binary={} pid={} key={}",
            binary, pid, bridge_key
        );

        // Background task: wait for the process to exit (prevents zombie processes) and
        // removes the PID from the active map when done.
        let active_bridges = Arc::clone(&self.active_bridges);
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    info!("bridge exited: key={} status={}", bridge_key, status)
                }
                Err(e) => error!("bridge wait error: key={} err={}", bridge_key, e),
            }
            active_bridges.lock().await.remove(&bridge_key);
        });
    }
}

/// Extract LiveKit credentials, mint a token, and assemble a `BridgeSpawnConfig`.
///
/// Returns `None` (after logging) when any required config field is absent or token
/// generation fails, signalling `try_spawn_bridge` to skip the spawn silently.
fn build_bridge_spawn_config(
    config: &DaemonConfig,
    target: &ScreenSharingTarget,
    username: String,
    password: String,
    bridge_identity: &str,
    track_name: &str,
    livekit_room: &str,
) -> Option<BridgeSpawnConfig> {
    let lk = match config.livekit.as_ref() {
        Some(lk) => lk,
        None => {
            info!("bridge spawn skipped: LiveKit not configured");
            return None;
        }
    };

    macro_rules! require_field {
        ($opt:expr, $msg:literal) => {
            match $opt.as_deref().filter(|s| !s.is_empty()) {
                Some(v) => v.to_string(),
                None => {
                    info!($msg);
                    return None;
                }
            }
        };
    }

    let livekit_url_internal =
        require_field!(lk.url, "bridge spawn skipped: LiveKit URL not configured");
    let api_key = require_field!(
        lk.api_key,
        "bridge spawn skipped: LiveKit API key not configured"
    );
    let api_secret = require_field!(
        lk.api_secret,
        "bridge spawn skipped: LiveKit API secret not configured"
    );

    let token = tddy_livekit::token::TokenGenerator::new(
        api_key,
        api_secret,
        livekit_room.to_string(),
        bridge_identity.to_string(),
        std::time::Duration::from_secs(tddy_livekit::token::DEFAULT_LIVEKIT_JWT_TTL_SECS),
    )
    .generate()
    .map_err(|e| {
        error!(
            "bridge spawn skipped: failed to generate LiveKit token: {}",
            e
        )
    })
    .ok()?;

    Some(BridgeSpawnConfig {
        host: target.host.clone(),
        port: target.port,
        username,
        password,
        livekit_url: livekit_url_internal,
        livekit_token: token,
        livekit_room: livekit_room.to_string(),
        livekit_identity: bridge_identity.to_string(),
        track_name: track_name.to_string(),
        width: DEFAULT_STREAM_WIDTH,
        height: DEFAULT_STREAM_HEIGHT,
        target_id: target.id.clone(),
        fps: DEFAULT_STREAM_FPS,
    })
}

fn vault_target_to_proto(t: &ScreenSharingTarget) -> ProtoScreenSharingTarget {
    ProtoScreenSharingTarget {
        id: t.id.clone(),
        label: t.label.clone(),
        host: t.host.clone(),
        port: t.port as u32,
        protocol: t.protocol as i32,
        username: t.username.clone(),
    }
}

#[async_trait]
impl ScreenSharingService for ScreenSharingServiceImpl {
    async fn list_targets(
        &self,
        request: Request<ListTargetsRequest>,
    ) -> Result<Response<ListTargetsResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        if !ScreenSharingVault::exists(&vault_file) {
            return Ok(Response::new(ListTargetsResponse { targets: vec![] }));
        }

        let targets = {
            let cache = self.key_cache.lock().await;
            if let Some(key) = cache.get(&req.session_id).cloned() {
                drop(cache);
                let (vault, _) = ScreenSharingVault::load_with_key(&vault_file, &key)
                    .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;
                vault.list_targets()
            } else {
                drop(cache);
                ScreenSharingVault::list_targets_from_file(&vault_file).map_err(|e| {
                    Status::internal(format!("failed to read vault metadata: {}", e))
                })?
            }
        };

        Ok(Response::new(ListTargetsResponse {
            targets: targets.iter().map(vault_target_to_proto).collect(),
        }))
    }

    async fn add_target(
        &self,
        request: Request<AddTargetRequest>,
    ) -> Result<Response<AddTargetResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = self.require_key(&req.session_id).await?;

        let (mut vault, _) = ScreenSharingVault::load_with_key(&vault_file, &key)
            .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;

        let protocol = Protocol::try_from(req.protocol).unwrap_or(Protocol::Unspecified);

        let target = vault
            .add_target(
                &req.label,
                &req.host,
                req.port as u16,
                &req.username,
                &req.password,
                protocol,
                &key,
            )
            .map_err(|e| Status::internal(format!("failed to add target: {}", e)))?;

        Ok(Response::new(AddTargetResponse {
            target: Some(vault_target_to_proto(&target)),
        }))
    }

    async fn remove_target(
        &self,
        request: Request<RemoveTargetRequest>,
    ) -> Result<Response<RemoveTargetResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = self.require_key(&req.session_id).await?;

        let (mut vault, _) = ScreenSharingVault::load_with_key(&vault_file, &key)
            .map_err(|e| Status::internal(format!("failed to load vault: {}", e)))?;

        vault
            .remove_target(&req.target_id)
            .map_err(|e| Status::not_found(format!("target not found: {}", e)))?;

        Ok(Response::new(RemoveTargetResponse { ok: true }))
    }

    async fn unlock_vault(
        &self,
        request: Request<UnlockVaultRequest>,
    ) -> Result<Response<UnlockVaultResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = if ScreenSharingVault::exists(&vault_file) {
            let (_vault, key) = ScreenSharingVault::unlock(&vault_file, &req.passphrase)
                .map_err(|_| Status::unauthenticated("invalid passphrase"))?;
            key
        } else {
            let (_vault, key) = ScreenSharingVault::create(&vault_file, &req.passphrase)
                .map_err(|e| Status::internal(format!("failed to create vault: {}", e)))?;
            key
        };

        let mut key_cache = self.key_cache.lock().await;
        key_cache.insert(req.session_id, key);
        drop(key_cache);

        Ok(Response::new(UnlockVaultResponse { ok: true }))
    }

    async fn start_stream(
        &self,
        request: Request<StartStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.resolve_session_dir(&req.session_token, &req.session_id)?;
        let vault_file = vault_path(&session_dir);

        let key = self.require_key(&req.session_id).await?;

        let metadata = tddy_core::session_metadata::read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("failed to read session metadata: {}", e)))?;
        let livekit_room = metadata.livekit_room.unwrap_or_default();

        let bridge_identity = format!("screenshare-{}-{}", req.session_id, req.target_id);
        let track_name = format!("screenshare:{}", req.target_id);

        // Browser-facing LiveKit URL (public_url preferred over internal url).
        let livekit_url = self
            .config
            .as_ref()
            .and_then(|c| c.livekit.as_ref())
            .map(|lk| {
                lk.public_url
                    .as_deref()
                    .or(lk.url.as_deref())
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default();

        // Spawn the bridge process when daemon config is available.
        if let Some(ref config) = self.config {
            match ScreenSharingVault::load_with_key(&vault_file, &key) {
                Ok((vault, _)) => {
                    let targets = vault.list_targets();
                    if let Some(target) = targets.into_iter().find(|t| t.id == req.target_id) {
                        let username = target.username.clone();
                        let password = vault
                            .decrypt_password(&req.target_id, &key)
                            .unwrap_or_default();
                        self.try_spawn_bridge(
                            config,
                            &target,
                            username,
                            password,
                            &bridge_identity,
                            &track_name,
                            &livekit_room,
                            &req.session_id,
                        )
                        .await;
                    } else {
                        error!(
                            "start_stream: target '{}' not found in vault",
                            req.target_id
                        );
                    }
                }
                Err(e) => {
                    error!("start_stream: failed to load vault: {}", e);
                }
            }
        }

        Ok(Response::new(StartStreamResponse {
            livekit_room,
            livekit_url,
            bridge_identity,
            track_name,
            width: DEFAULT_STREAM_WIDTH,
            height: DEFAULT_STREAM_HEIGHT,
        }))
    }

    async fn stop_stream(
        &self,
        request: Request<StopStreamRequest>,
    ) -> Result<Response<StopStreamResponse>, Status> {
        let req = request.into_inner();
        let bridge_key = format!("{}:{}", req.session_id, req.target_id);

        let pid_opt = self.active_bridges.lock().await.remove(&bridge_key);

        if let Some(pid) = pid_opt {
            info!(
                "stop_stream: sending SIGTERM to bridge pid={} key={}",
                pid, bridge_key
            );
            #[cfg(unix)]
            // SAFETY: pid > 0 is enforced by the `if pid > 0` guard in `try_spawn_bridge`
            // that filters out the zero sentinel before inserting into `active_bridges`.
            // Sending SIGTERM to a pid is safe — the signal is delivered to the process
            // group and the call does not affect memory safety.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        } else {
            info!(
                "stop_stream: no active bridge for key={} (already stopped?)",
                bridge_key
            );
        }

        Ok(Response::new(StopStreamResponse { ok: true }))
    }
}
