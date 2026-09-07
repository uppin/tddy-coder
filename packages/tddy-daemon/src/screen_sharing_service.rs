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
use crate::host_desktop_targets::{HostDesktopTarget, HostDesktopTargetStore};
use crate::host_keypair::HostKeypair;
use crate::screen_sharing_vault::{
    vault_path, DerivedKey, ScreenSharingTarget, ScreenSharingVault,
};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddHostTargetResponse, AddTargetRequest, AddTargetResponse,
    ListHostTargetsRequest, ListHostTargetsResponse, ListTargetsRequest, ListTargetsResponse,
    Protocol, RemoveHostTargetRequest, RemoveHostTargetResponse, RemoveTargetRequest,
    RemoveTargetResponse, ScreenSharingService, ScreenSharingTarget as ProtoScreenSharingTarget,
    StartHostStreamRequest, StartStreamRequest, StartStreamResponse, StopHostStreamRequest,
    StopHostStreamResponse as HostStopStreamResponse, StopStreamRequest, StopStreamResponse,
    UnlockVaultRequest, UnlockVaultResponse,
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
/// Wired in `runtime.rs` with the daemon's resolved `tddy_data_dir` (config-only tddy home) so
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

/// What the host-scoped calls need, and the session-scoped ones do not.
///
/// One value rather than two independent options: a daemon that can store a host's desktop but
/// cannot read the password prompted for it can start nothing, so "host scope is wired" is a
/// single fact.
struct HostScope {
    targets: Arc<dyn HostDesktopTargetStore>,
    /// `#hosts-screen 6/8`'s keypair, reused rather than reimplemented — one key per host, and one
    /// decryption path for everything the browser encrypts to it.
    keypair: Arc<dyn HostKeypair>,
}

/// Daemon-side implementation of `ScreenSharingService`.
pub struct ScreenSharingServiceImpl {
    user_resolver: UserResolver,
    sessions_base: SessionsBase,
    key_cache: ScreenSharingKeyCache,
    /// Optional daemon config — when set, `start_stream` spawns real bridge processes.
    config: Option<Arc<DaemonConfig>>,
    /// Active bridge PIDs keyed by [`session_bridge_key`] or [`host_bridge_key`], for the matching
    /// stop call.
    active_bridges: Arc<Mutex<HashMap<String, u32>>>,
    /// Absent until the daemon wires host scope; the host-scoped calls report that rather than
    /// pretending a host has no desktops.
    host_scope: Option<HostScope>,
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
            host_scope: None,
        }
    }

    /// Supply daemon config for bridge binary path resolution and LiveKit token generation.
    pub fn with_config(mut self, config: Arc<DaemonConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// Supply the store a host's desktops live in and the keypair their passwords are encrypted
    /// under, enabling the host-scoped calls.
    pub fn with_host_scope(
        mut self,
        targets: Arc<dyn HostDesktopTargetStore>,
        keypair: Arc<dyn HostKeypair>,
    ) -> Self {
        self.host_scope = Some(HostScope { targets, keypair });
        self
    }

    /// The OS user a session token belongs to.
    fn require_user(&self, session_token: &str) -> Result<String, Status> {
        (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid session token"))
    }

    fn require_host_scope(&self) -> Result<&HostScope, Status> {
        self.host_scope.as_ref().ok_or_else(|| {
            Status::failed_precondition("this daemon serves no host-scoped desktop targets")
        })
    }

    fn require_config(&self) -> Result<&DaemonConfig, Status> {
        self.config.as_deref().ok_or_else(|| {
            Status::failed_precondition("this daemon has no configuration to reach LiveKit with")
        })
    }

    fn resolve_session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        let user = self.require_user(session_token)?;
        let base = (self.sessions_base)(&user)
            .ok_or_else(|| Status::internal("sessions base not found for user"))?;
        Ok(base.join("sessions").join(session_id))
    }

    /// Signal a running bridge to stop and forget its PID.
    ///
    /// Shared by both scopes: the process, the signal and the PID map are the same, and only the
    /// key that addresses it differs.
    async fn terminate_bridge(&self, bridge_key: &str) {
        let pid_opt = self.active_bridges.lock().await.remove(bridge_key);

        if let Some(pid) = pid_opt {
            info!(
                "stop stream: sending SIGTERM to bridge pid={} key={}",
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
                "stop stream: no active bridge for key={} (already stopped?)",
                bridge_key
            );
        }
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
        // Built by `session_bridge_key` or `host_bridge_key`: the scope decides how a running
        // bridge is addressed, the spawn itself does not care which.
        bridge_key: String,
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

        // Store the PID so the matching stop call can send SIGTERM.
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

/// How a session's running bridge is addressed in `active_bridges`.
fn session_bridge_key(session_id: &str, target_id: &str) -> String {
    format!("{}:{}", session_id, target_id)
}

/// How a host's running bridge is addressed in `active_bridges`.
///
/// Prefixed, so a daemon instance id can never collide with a session id and stop the wrong
/// bridge — the two id spaces are unrelated and neither is reserved against the other.
fn host_bridge_key(daemon_instance_id: &str, target_id: &str) -> String {
    format!("host:{}:{}", daemon_instance_id, target_id)
}

/// The identity the host's bridge joins LiveKit under, and the one the overlay subscribes to.
///
/// Carries the host id because every host's bridge publishes into the *same* common room; without
/// it, two hosts streaming at once would collide on one identity.
fn host_bridge_identity(daemon_instance_id: &str, target_id: &str) -> String {
    format!("screenshare-host-{}-{}", daemon_instance_id, target_id)
}

fn screenshare_track_name(target_id: &str) -> String {
    format!("screenshare:{}", target_id)
}

/// The LiveKit URL to hand the **browser** — `public_url` when the operator set one, since the
/// internal URL may not resolve outside the daemon's network.
fn browser_livekit_url(config: &DaemonConfig) -> String {
    config
        .livekit
        .as_ref()
        .map(|lk| {
            lk.public_url
                .as_deref()
                .or(lk.url.as_deref())
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

/// The room a **host** desktop is published into: this daemon's common room.
///
/// A session desktop has a room of its own, named in the session's metadata. A host has none — and
/// the viewer opening it from the Hosts screen is already in the common room, which is the one
/// room a browser holds a token for without opening a session.
///
/// Deliberately not gated on `livekit.enabled`: that switch governs whether *this daemon* joins the
/// common room, and a bridge joins as a participant of its own. What it cannot do without is a room
/// name, and a daemon without one says so rather than starting a bridge nobody could watch.
fn host_livekit_room(config: &DaemonConfig) -> Result<String, Status> {
    config
        .livekit
        .as_ref()
        .and_then(|lk| lk.common_room.as_deref())
        .filter(|room| !room.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            Status::failed_precondition(
                "no LiveKit common room is configured; a host desktop has no room to publish into",
            )
        })
}

/// A desktop password for immediate use, from the ciphertext the browser encrypted under this
/// host's published key.
///
/// The plaintext is owned by the caller and dies with the call that asked for it: `#hosts-screen
/// 6/8`'s prompt-decrypt-drop posture, deliberately not the session vault's storing one, so the
/// Hosts screen has a single secret-handling model. Nothing here writes it anywhere.
fn decrypt_desktop_password(
    keypair: &dyn HostKeypair,
    encrypted_password: &[u8],
) -> Result<String, Status> {
    if encrypted_password.is_empty() {
        // A desktop that asks for no password: the bridges take an empty one.
        return Ok(String::new());
    }
    let plaintext = keypair
        .decrypt(encrypted_password)
        .map_err(Status::permission_denied)?;
    String::from_utf8(plaintext)
        .map_err(|_| Status::invalid_argument("that desktop password is not valid UTF-8"))
}

/// A host-scoped target as the shared bridge spawn path's input.
///
/// Converted rather than given a spawn path of its own: a second implementation would be free to
/// drift from the one the session scope is proven against, and host scope is an addressing change,
/// not a second way to start a bridge.
fn host_target_as_bridge_target(target: &HostDesktopTarget) -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: target.target_id.clone(),
        label: target.label.clone(),
        host: target.host.clone(),
        port: target.port,
        protocol: Protocol::try_from(target.protocol).unwrap_or(Protocol::Unspecified),
        username: target.username.clone(),
    }
}

fn host_target_to_proto(t: &HostDesktopTarget) -> ProtoScreenSharingTarget {
    ProtoScreenSharingTarget {
        id: t.target_id.clone(),
        label: t.label.clone(),
        host: t.host.clone(),
        port: t.port as u32,
        protocol: t.protocol,
        username: t.username.clone(),
    }
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
    // --- Host-scoped targets (#hosts-screen 8/8) ---
    //
    // A desktop belongs to a machine, not to a coding session. These mirror the session-scoped
    // calls above, addressed by `daemon_instance_id`, and reuse the same bridge spawn path — host
    // scope is an addressing and storage change, not a second implementation.

    async fn list_host_targets(
        &self,
        request: Request<ListHostTargetsRequest>,
    ) -> Result<Response<ListHostTargetsResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;

        let targets = scope.targets.list(&req.daemon_instance_id);

        Ok(Response::new(ListHostTargetsResponse {
            targets: targets.iter().map(host_target_to_proto).collect(),
        }))
    }

    async fn add_host_target(
        &self,
        request: Request<AddHostTargetRequest>,
    ) -> Result<Response<AddHostTargetResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;

        // No password field, and none accepted: a host desktop's password is prompted when it is
        // opened, never stored beside the address of the machine it opens.
        let target_id = scope
            .targets
            .add(
                &req.daemon_instance_id,
                HostDesktopTarget {
                    // Assigned by the store; whatever is sent here would be overwritten.
                    target_id: String::new(),
                    label: req.label,
                    host: req.host,
                    port: req.port as u16,
                    protocol: req.protocol,
                    username: req.username,
                },
            )
            .map_err(Status::internal)?;

        Ok(Response::new(AddHostTargetResponse { target_id }))
    }

    async fn remove_host_target(
        &self,
        request: Request<RemoveHostTargetRequest>,
    ) -> Result<Response<RemoveHostTargetResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;

        scope
            .targets
            .remove(&req.daemon_instance_id, &req.target_id)
            .map_err(Status::not_found)?;

        Ok(Response::new(RemoveHostTargetResponse { ok: true }))
    }

    /// Starts a bridge for a host-scoped target, returning the same coordinates the session-scoped
    /// call does — the browser overlay already consumes exactly those fields.
    async fn start_host_stream(
        &self,
        request: Request<StartHostStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;
        let config = self.require_config()?;

        let target = scope
            .targets
            .list(&req.daemon_instance_id)
            .into_iter()
            .find(|t| t.target_id == req.target_id)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "host {} has no desktop target {}",
                    req.daemon_instance_id, req.target_id
                ))
            })?;

        let livekit_room = host_livekit_room(config)?;
        let bridge_identity = host_bridge_identity(&req.daemon_instance_id, &req.target_id);
        let track_name = screenshare_track_name(&req.target_id);

        // Read once, handed to the bridge over its stdin, and dropped when this call returns.
        let password = decrypt_desktop_password(scope.keypair.as_ref(), &req.encrypted_password)?;

        self.try_spawn_bridge(
            config,
            &host_target_as_bridge_target(&target),
            target.username.clone(),
            password,
            &bridge_identity,
            &track_name,
            &livekit_room,
            host_bridge_key(&req.daemon_instance_id, &req.target_id),
        )
        .await;

        Ok(Response::new(StartStreamResponse {
            livekit_room,
            livekit_url: browser_livekit_url(config),
            bridge_identity,
            track_name,
            width: DEFAULT_STREAM_WIDTH,
            height: DEFAULT_STREAM_HEIGHT,
        }))
    }

    async fn stop_host_stream(
        &self,
        request: Request<StopHostStreamRequest>,
    ) -> Result<Response<HostStopStreamResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;

        self.terminate_bridge(&host_bridge_key(&req.daemon_instance_id, &req.target_id))
            .await;

        Ok(Response::new(HostStopStreamResponse { ok: true }))
    }

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
        let track_name = screenshare_track_name(&req.target_id);

        let livekit_url = self
            .config
            .as_deref()
            .map(browser_livekit_url)
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
                            session_bridge_key(&req.session_id, &req.target_id),
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

        self.terminate_bridge(&session_bridge_key(&req.session_id, &req.target_id))
            .await;

        Ok(Response::new(StopStreamResponse { ok: true }))
    }
}

/// Host-scoped start and stop, against a stand-in bridge binary.
///
/// `unix`, like the daemon's own stop path: the stub is a `/bin/sh` script and "the bridge was
/// released" is proven by the pid dying on `SIGTERM`. Both are POSIX, and neither has a meaning to
/// assert on a platform without them.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::path::Path;
    use std::time::Duration;

    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };
    use tddy_service::proto::screen_sharing::AddHostTargetRequest;
    use tddy_testing_commons::process_is_alive;
    use tddy_testing_commons::stub_scripts::{make_executable, read_recorded_argv};
    use tddy_testing_commons::wait::eventually;

    use crate::config::{LiveKitConfig, ScreenSharingConfig};
    use crate::host_desktop_targets::FileHostDesktopTargetStore;
    use crate::host_keypair::{FileHostKeypair, PublishedKey};

    const A_SESSION_TOKEN: &str = "a-valid-session-token";
    const THE_OS_USER: &str = "ada";
    const A_HOST: &str = "workstation-1";
    const A_SESSION: &str = "session-with-a-desktop";
    const THE_COMMON_ROOM: &str = "tddy-hosts-lobby";
    const THE_SESSIONS_OWN_ROOM: &str = "room-of-one-session";
    /// Reachable from the daemon, not from a browser — which is the whole reason a public URL
    /// exists alongside it.
    const THE_INTERNAL_LIVEKIT_URL: &str = "ws://livekit.internal:7880";
    const THE_BROWSER_LIVEKIT_URL: &str = "wss://livekit.example.com";
    const A_DESKTOP_PASSWORD: &str = "correct horse battery staple";
    const A_VAULT_PASSPHRASE: &str = "hunter2-passphrase";

    /// Safety nets, not predictions: a stub that has recorded nothing by now was never spawned, and
    /// a signalled process still alive by now was never signalled. Both cost nothing when the
    /// condition already holds.
    const A_BRIDGE_RECORDS_ITS_INVOCATION_WITHIN: Duration = Duration::from_secs(10);
    const A_SIGNALLED_BRIDGE_EXITS_WITHIN: Duration = Duration::from_secs(10);

    /// Longer than any test here runs, so a bridge only ever exits because it was released.
    const A_BRIDGE_STAYS_UP_FOR_SECS: u32 = 300;

    /// A stand-in for `tddy-vnc`: records how the daemon invoked it, then stays alive until it is
    /// signalled.
    ///
    /// `exec sleep` rather than a plain `sleep`, deliberately. What a stop has to prove is that the
    /// **recorded pid** is gone, so that pid must belong to the process that is still running when
    /// the signal arrives. A shell that forked a `sleep` would die on the signal while leaving that
    /// `sleep` orphaned behind every run, and the assertion would be about the wrong process.
    struct FakeBridge {
        binary: PathBuf,
        argv_file: PathBuf,
        stdin_file: PathBuf,
    }

    impl FakeBridge {
        fn installed_in(dir: &Path) -> Self {
            let bridge = Self {
                binary: dir.join("tddy-vnc-stub"),
                argv_file: dir.join("bridge-argv"),
                stdin_file: dir.join("bridge-stdin.json"),
            };
            let (argv, stdin) = (bridge.argv_file.display(), bridge.stdin_file.display());
            // Each record is written to a temp file and `mv -f`'d over the target, the way
            // `stub_scripts` does it: a reader must see a whole record or none, never a
            // half-written one that would read as the daemon having built the wrong invocation.
            std::fs::write(
                &bridge.binary,
                format!(
                    "#!/bin/sh\n\
                     printf '%s\\n' \"$0\" \"$@\" > \"{argv}.part\"\n\
                     mv -f \"{argv}.part\" \"{argv}\"\n\
                     cat > \"{stdin}.part\"\n\
                     mv -f \"{stdin}.part\" \"{stdin}\"\n\
                     exec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n"
                ),
            )
            .expect("writing the stand-in bridge");
            make_executable(&bridge.binary);
            bridge
        }

        /// The command line the daemon invoked the bridge with, `$0` first.
        async fn recorded_argv(&self) -> Vec<String> {
            eventually(
                "the bridge records the command line it was invoked with",
                A_BRIDGE_RECORDS_ITS_INVOCATION_WITHIN,
                || read_recorded_argv(&self.argv_file),
            )
            .await
        }

        /// The `BridgeConfig` JSON the daemon wrote to the bridge's stdin.
        async fn config_read_from_its_stdin(&self) -> serde_json::Value {
            eventually(
                "the bridge reads its config from stdin",
                A_BRIDGE_RECORDS_ITS_INVOCATION_WITHIN,
                || {
                    let written = std::fs::read(&self.stdin_file).map_err(|e| {
                        format!("{} is not readable yet: {e}", self.stdin_file.display())
                    })?;
                    serde_json::from_slice(&written).map_err(|e| {
                        format!("{} is not whole JSON yet: {e}", self.stdin_file.display())
                    })
                },
            )
            .await
        }
    }

    /// The service wired the way `runtime.rs` wires it — real target store, real keypair, real
    /// vault — over throwaway directories, with the bridge binary pointed at [`FakeBridge`].
    ///
    /// Everything below the RPC is the production code path; only the desktop at the far end is
    /// stood in for, because a bridge cannot connect to a machine that is not there.
    struct DaemonUnderTest {
        service: ScreenSharingServiceImpl,
        bridge: FakeBridge,
        keypair: Arc<FileHostKeypair>,
        storage: tempfile::TempDir,
    }

    fn a_daemon() -> DaemonUnderTest {
        let storage = tempfile::tempdir().expect("a storage directory");
        let bridge = FakeBridge::installed_in(storage.path());

        let config = Arc::new(DaemonConfig {
            livekit: Some(LiveKitConfig {
                url: Some(THE_INTERNAL_LIVEKIT_URL.to_string()),
                public_url: Some(THE_BROWSER_LIVEKIT_URL.to_string()),
                api_key: Some("an-api-key".to_string()),
                api_secret: Some("an-api-secret".to_string()),
                common_room: Some(THE_COMMON_ROOM.to_string()),
                ..Default::default()
            }),
            screen_sharing: Some(ScreenSharingConfig {
                vnc_binary_path: bridge.binary.display().to_string(),
                ..Default::default()
            }),
            ..Default::default()
        });

        let sessions_base = storage.path().to_path_buf();
        let keypair = Arc::new(FileHostKeypair::new(storage.path()));
        let targets = Arc::new(FileHostDesktopTargetStore::new(storage.path()));

        let service = ScreenSharingServiceImpl::new(
            Arc::new(|token: &str| (token == A_SESSION_TOKEN).then(|| THE_OS_USER.to_string())),
            Arc::new(move |_user: &str| Some(sessions_base.clone())),
            Arc::new(Mutex::new(HashMap::new())),
        )
        .with_config(config)
        .with_host_scope(targets, Arc::clone(&keypair) as Arc<dyn HostKeypair>);

        DaemonUnderTest {
            service,
            bridge,
            keypair,
            storage,
        }
    }

    impl DaemonUnderTest {
        /// Attach a VNC desktop to `host`, as the Hosts screen does before opening one.
        async fn attach_a_desktop_to(&self, host: &str) -> String {
            self.service
                .add_host_target(Request::new(AddHostTargetRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    label: "dev box".to_string(),
                    host: "127.0.0.1".to_string(),
                    port: 5900,
                    protocol: Protocol::Vnc as i32,
                    username: "ada".to_string(),
                }))
                .await
                .expect("attaching a desktop to a host")
                .into_inner()
                .target_id
        }

        async fn open_the_desktop_of(&self, host: &str, target_id: &str) -> StartStreamResponse {
            self.open_the_desktop_of_with(host, target_id, Vec::new())
                .await
        }

        async fn open_the_desktop_of_with(
            &self,
            host: &str,
            target_id: &str,
            encrypted_password: Vec<u8>,
        ) -> StartStreamResponse {
            self.service
                .start_host_stream(Request::new(StartHostStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                    encrypted_password,
                }))
                .await
                .expect("opening a host's desktop")
                .into_inner()
        }

        async fn close_the_desktop_of(&self, host: &str, target_id: &str) {
            self.service
                .stop_host_stream(Request::new(StopHostStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .expect("closing a host's desktop");
        }

        /// A desktop password as the browser sends it: RSA-OAEP(SHA-256) under this host's
        /// published key, which is the only form `StartHostStream` accepts.
        fn encrypted_for_this_host(&self, password: &str) -> Vec<u8> {
            use rsa::pkcs8::DecodePublicKey;

            let PublishedKey { spki_der, .. } =
                self.keypair.published().expect("this host's published key");
            rsa::RsaPublicKey::from_public_key_der(&spki_der)
                .expect("a published SPKI DER public key")
                .encrypt(
                    &mut rand::thread_rng(),
                    rsa::Oaep::new::<sha2::Sha256>(),
                    password.as_bytes(),
                )
                .expect("a password fits comfortably in an OAEP payload")
        }

        async fn bridge_running_for(&self, bridge_key: &str) -> Option<u32> {
            self.service
                .active_bridges
                .lock()
                .await
                .get(bridge_key)
                .copied()
        }

        /// A session holding one password-less desktop in its unlocked vault — the per-session
        /// path exactly as it exists today.
        async fn a_session_holding_a_desktop(&self) -> String {
            let session_dir = self.storage.path().join("sessions").join(A_SESSION);
            std::fs::create_dir_all(&session_dir).expect("a session directory");
            write_initial_tool_session_metadata(
                &session_dir,
                InitialToolSessionMetadataOpts {
                    project_id: "a-project".to_string(),
                    livekit_room: Some(THE_SESSIONS_OWN_ROOM.to_string()),
                    ..Default::default()
                },
            )
            .expect("a session's metadata");

            self.service
                .unlock_vault(Request::new(UnlockVaultRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    session_id: A_SESSION.to_string(),
                    passphrase: A_VAULT_PASSPHRASE.to_string(),
                }))
                .await
                .expect("unlocking the session's vault");

            self.service
                .add_target(Request::new(AddTargetRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    session_id: A_SESSION.to_string(),
                    label: "dev box".to_string(),
                    host: "127.0.0.1".to_string(),
                    port: 5900,
                    password: String::new(),
                    protocol: Protocol::Vnc as i32,
                    username: "ada".to_string(),
                }))
                .await
                .expect("adding a desktop to the session's vault")
                .into_inner()
                .target
                .expect("the added target")
                .id
        }

        async fn open_the_session_desktop(&self, target_id: &str) -> StartStreamResponse {
            self.service
                .start_stream(Request::new(StartStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    session_id: A_SESSION.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .expect("opening a session's desktop")
                .into_inner()
        }
    }

    impl Drop for DaemonUnderTest {
        /// A test that leaves a sleeping stand-in bridge behind is a bad neighbour to every run
        /// after it, so anything still tracked when the daemon goes away is killed outright.
        fn drop(&mut self) {
            let Ok(bridges) = self.service.active_bridges.try_lock() else {
                return;
            };
            for pid in bridges.values() {
                // SAFETY: these are pids `try_spawn_bridge` recorded, all greater than zero, and
                // signalling a pid affects no memory in this process.
                unsafe { libc::kill(*pid as libc::pid_t, libc::SIGKILL) };
            }
        }
    }

    /// AC-2. The overlay mounts on exactly these six fields; a start that returned any other room,
    /// identity or track name would render nothing and say nothing about why.
    #[tokio::test]
    async fn starting_a_host_desktop_returns_the_room_identity_and_track_the_viewer_needs() {
        // Given a desktop attached to a host
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened
        let stream = daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // Then the reply is the host's coordinates, in the shape the session-scoped call returns
        assert_eq!(
            stream,
            StartStreamResponse {
                livekit_room: THE_COMMON_ROOM.to_string(),
                livekit_url: THE_BROWSER_LIVEKIT_URL.to_string(),
                bridge_identity: format!("screenshare-host-{A_HOST}-{target_id}"),
                track_name: format!("screenshare:{target_id}"),
                width: 1920,
                height: 1080,
            }
        );
    }

    /// AC-4. Closing the overlay must end the bridge, not merely forget it: a process per desktop
    /// anyone ever looked at is how a host runs out of memory.
    #[tokio::test]
    async fn stopping_a_host_desktop_releases_the_bridge_process() {
        // Given an open host desktop, with a bridge process running for it
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;
        daemon.open_the_desktop_of(A_HOST, &target_id).await;
        let pid = daemon
            .bridge_running_for(&host_bridge_key(A_HOST, &target_id))
            .await
            .expect("opening a desktop spawns a bridge and records its pid");
        assert!(
            process_is_alive(pid),
            "the bridge must be running before the stop, or this proves nothing"
        );

        // When the desktop is closed
        daemon.close_the_desktop_of(A_HOST, &target_id).await;

        // Then the process it was streaming through is gone
        eventually(
            "the bridge process exits after being released",
            A_SIGNALLED_BRIDGE_EXITS_WITHIN,
            || {
                if process_is_alive(pid) {
                    Err(format!("bridge pid {pid} is still running"))
                } else {
                    Ok(())
                }
            },
        )
        .await;
    }

    /// AC-10. Argv is world-readable on this machine — `ps` shows it to every local account — so a
    /// password on a bridge's command line is a password published to anyone logged in.
    #[tokio::test]
    async fn a_host_desktop_password_never_appears_in_the_bridge_process_arguments() {
        // Given a desktop attached to a host
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened with a password, encrypted under this host's key
        let encrypted_password = daemon.encrypted_for_this_host(A_DESKTOP_PASSWORD);
        daemon
            .open_the_desktop_of_with(A_HOST, &target_id, encrypted_password)
            .await;

        // Then the bridge was given it on stdin — the assertion that makes the next one mean
        // something, since a password that never reached the bridge is trivially not in its argv
        let config = daemon.bridge.config_read_from_its_stdin().await;
        assert_eq!(config["password"].as_str(), Some(A_DESKTOP_PASSWORD));

        // Then it appears nowhere on the command line
        let argv = daemon.bridge.recorded_argv().await;
        assert!(
            !argv.iter().any(|arg| arg.contains(A_DESKTOP_PASSWORD)),
            "the desktop password reached the bridge's argv: {argv:?}"
        );
    }

    /// AC-9. Host scope is an addressing change on a service the per-session feature already uses,
    /// so the likeliest defect in this node is not a broken host desktop — it is a session desktop
    /// that stopped working because the two paths now share a spawn, a PID map and a stop.
    #[tokio::test]
    async fn starting_a_session_desktop_still_works_unchanged() {
        // Given a session holding a desktop in its unlocked vault
        let daemon = a_daemon();
        let target_id = daemon.a_session_holding_a_desktop().await;

        // When that session's desktop is opened
        let stream = daemon.open_the_session_desktop(&target_id).await;

        // Then it streams from the session's own room, under the session's own identity
        assert_eq!(
            stream,
            StartStreamResponse {
                livekit_room: THE_SESSIONS_OWN_ROOM.to_string(),
                livekit_url: THE_BROWSER_LIVEKIT_URL.to_string(),
                bridge_identity: format!("screenshare-{A_SESSION}-{target_id}"),
                track_name: format!("screenshare:{target_id}"),
                width: 1920,
                height: 1080,
            }
        );
        // …and its bridge is tracked under the key the session path has always used, written out
        // here rather than asked of the production code, which would agree with itself whatever it
        // built
        assert!(
            daemon
                .bridge_running_for(&format!("{A_SESSION}:{target_id}"))
                .await
                .is_some(),
            "a session's bridge must stay addressable by its own session id"
        );
    }
}
