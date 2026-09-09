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
use crate::host_prompts::{answer_before_expiry, HostPromptRegistry, PromptKind};
use crate::screen_sharing_vault::{
    vault_path, DerivedKey, ScreenSharingTarget, ScreenSharingVault,
};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddHostTargetResponse, AddTargetRequest, AddTargetResponse,
    ListHostTargetsRequest, ListHostTargetsResponse, ListTargetsRequest, ListTargetsResponse,
    Protocol, RemoveTargetRequest, RemoveTargetResponse, ScreenSharingService,
    ScreenSharingTarget as ProtoScreenSharingTarget, StartHostStreamRequest, StartStreamRequest,
    StartStreamResponse, StopHostStreamRequest, StopHostStreamResponse, StopStreamRequest,
    StopStreamResponse, UnlockVaultRequest, UnlockVaultResponse,
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
    /// `#hosts-screen 6/8`'s prompt channel, on which this host asks its operator for a desktop
    /// password. The browser has no other way to learn this host's public key: `HostPromptEvent`
    /// is the only place it is published, so the prompt has to be **raised by the daemon**.
    ///
    /// The same instance `ConnectionService` streams and answers on — a prompt raised on one
    /// registry and answered on another is a question nobody can answer.
    prompts: Arc<dyn HostPromptRegistry>,
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

    /// Supply the store a host's desktops live in, the keypair their passwords are encrypted
    /// under, and the prompt channel those passwords are asked for on — enabling the host-scoped
    /// calls.
    ///
    /// All three together rather than separately: a daemon that can store a host's desktop but
    /// cannot ask for the password it needs can start nothing, so "host scope is wired" stays a
    /// single fact.
    pub fn with_host_scope(
        mut self,
        targets: Arc<dyn HostDesktopTargetStore>,
        keypair: Arc<dyn HostKeypair>,
        prompts: Arc<dyn HostPromptRegistry>,
    ) -> Self {
        self.host_scope = Some(HostScope {
            targets,
            keypair,
            prompts,
        });
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

    /// Ask this host's operator for `target`'s password, and return what they typed.
    ///
    /// The question is raised **here**, by the daemon, rather than answered by the caller. It has to
    /// be: `HostPromptEvent` is the only place a host publishes the public key an answer travels
    /// under, so a browser that has not been asked anything has nothing to encrypt with — and a
    /// desktop password is stored nowhere on this host, which is the whole point of prompting for
    /// it.
    ///
    /// Every start asks. Nothing records whether a desktop wants a password, and a daemon that
    /// guessed would either skip the question for a desktop that needs one or refuse a desktop that
    /// does not; an operator answering with nothing is how a password-less desktop is opened.
    ///
    /// The wait is bounded by the prompt's own expiry, so an operator who walks away releases the
    /// call at the moment the question stops being answerable — never later, and never never.
    async fn prompt_for_desktop_password(
        &self,
        scope: &HostScope,
        operator: &str,
        target: &HostDesktopTarget,
    ) -> Result<String, Status> {
        // Stamped with the GitHub user that raised it, which is what makes it *this* operator's
        // question: `StreamHostPrompts` shows a prompt to nobody else, and nobody else can spend
        // its one answer. Anything else here and the question reaches no browser at all.
        let prompt = scope.prompts.issue(
            operator,
            PromptKind::DesktopPassword,
            &desktop_prompt_subject(target),
            crate::host_registry::now_unix_ms(),
        );
        // Claimed immediately after issuing, because issuing is what puts the prompt on the feed: an
        // operator whose browser answers at once must find a handoff already waiting for them.
        let answer = match scope.prompts.awaited_answer(&prompt.prompt_id) {
            Some(handoff) => answer_before_expiry(handoff, &prompt).await,
            // The registry forgot the prompt between issuing it and being asked for its handoff,
            // which for the operator is indistinguishable from one that ran out of time.
            None => None,
        };
        let Some(encrypted_password) = answer else {
            // Refused rather than started without one: a bridge spawned against a desktop whose
            // password nobody supplied authenticates to nothing, and would leave a process running
            // for a stream that can never carry a frame.
            return Err(Status::deadline_exceeded(format!(
                "nobody answered the password prompt for {} before it expired",
                prompt.subject
            )));
        };
        decrypt_desktop_password(scope.keypair.as_ref(), &encrypted_password)
    }

    /// Spawn a bridge for the given target, logging every failure and reporting none.
    ///
    /// The **session-scoped** spawn, and deliberately silent: `start_stream` hands back its
    /// pre-computed LiveKit coordinates whether or not a bridge came up, which is the behaviour
    /// AC-9 pins and its callers depend on. The host path does not reuse this — it reports —
    /// see [`Self::spawn_bridge`] and `start_host_stream`.
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
        let prepared = match prepare_bridge(
            config,
            target,
            username,
            bridge_identity,
            track_name,
            livekit_room,
        ) {
            Ok(prepared) => prepared.for_password(password),
            Err(e) => {
                info!("bridge spawn skipped: {e}");
                return;
            }
        };

        if let Err(e) = self.spawn_bridge(prepared, bridge_key).await {
            error!("bridge spawn failed: {e}");
        }
    }

    /// Start a bridge process, and say so when it does not start.
    ///
    /// Takes a [`PreparedBridge`], so everything knowable without the desktop's password has
    /// already been settled: what is left here is the spawn itself, and the only failures it can
    /// report are real ones.
    async fn spawn_bridge(
        &self,
        prepared: PreparedBridge,
        // Built by `session_bridge_key` or `host_bridge_key`: the scope decides how a running
        // bridge is addressed, the spawn itself does not care which.
        bridge_key: String,
    ) -> Result<(), String> {
        let PreparedBridge {
            binary,
            spawn_config,
        } = prepared;

        let config_json = serde_json::to_vec(&spawn_config)
            .map_err(|e| format!("encoding the bridge's config: {e}"))?;

        // A second start for the same desktop must not strand the first process. `active_bridges`
        // holds one pid per key, so a spawn that overwrote one would leave a bridge nothing can
        // ever signal — running against a desktop nobody is watching.
        self.terminate_bridge(&bridge_key).await;

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawning the bridge binary '{binary}': {e}"))?;

        // Write the JSON config to the bridge's stdin, then close it.
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(&config_json).await {
                let _ = child.kill().await;
                return Err(format!("writing the bridge's config to its stdin: {e}"));
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
        // forget the PID when it does.
        let active_bridges = Arc::clone(&self.active_bridges);
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    info!("bridge exited: key={} status={}", bridge_key, status)
                }
                Err(e) => error!("bridge wait error: key={} err={}", bridge_key, e),
            }
            // Only while the key still names *this* bridge. A restart records a newer pid under
            // the same key, and forgetting that one would leave the running process unstoppable.
            let mut bridges = active_bridges.lock().await;
            if bridges.get(&bridge_key) == Some(&pid) {
                bridges.remove(&bridge_key);
            }
        });

        Ok(())
    }
}

/// A bridge spawn that is ready except for the password nobody has typed yet.
///
/// Separated from the spawn so that everything knowable *without* a secret — the LiveKit
/// coordinates, the minted token, which binary speaks this desktop's protocol — is settled before
/// an operator is asked for one. A desktop this host could never bridge is then refused without
/// anybody typing a password for a stream that was never going to start.
struct PreparedBridge {
    binary: String,
    spawn_config: BridgeSpawnConfig,
}

impl PreparedBridge {
    fn for_password(mut self, password: String) -> Self {
        self.spawn_config.password = password;
        self
    }
}

/// Everything a bridge spawn needs except the desktop's password.
fn prepare_bridge(
    config: &DaemonConfig,
    target: &ScreenSharingTarget,
    username: String,
    bridge_identity: &str,
    track_name: &str,
    livekit_room: &str,
) -> Result<PreparedBridge, String> {
    let binary = match target.protocol {
        Protocol::Vnc => resolve_vnc_binary_path(config),
        Protocol::Rdp => resolve_rdp_binary_path(config),
        Protocol::Unspecified => {
            return Err(format!(
                "target {} names no protocol to bridge it with",
                target.id
            ))
        }
    };

    Ok(PreparedBridge {
        binary,
        spawn_config: build_bridge_spawn_config(
            config,
            target,
            username,
            bridge_identity,
            track_name,
            livekit_room,
        )?,
    })
}

/// Extract LiveKit credentials, mint a token, and assemble a `BridgeSpawnConfig`.
///
/// Says what is missing rather than logging it: a host desktop that cannot be published anywhere
/// has to be refused, and the caller is the only place that knows whether that refusal reaches an
/// operator or only the log.
///
/// The password is left empty here and filled in by [`PreparedBridge::for_password`] — nothing in
/// this function needs one, which is exactly why it can run before anybody is asked.
fn build_bridge_spawn_config(
    config: &DaemonConfig,
    target: &ScreenSharingTarget,
    username: String,
    bridge_identity: &str,
    track_name: &str,
    livekit_room: &str,
) -> Result<BridgeSpawnConfig, String> {
    let lk = config
        .livekit
        .as_ref()
        .ok_or("this daemon has no LiveKit configuration to publish a desktop through")?;

    macro_rules! require_field {
        ($opt:expr, $msg:literal) => {
            match $opt.as_deref().filter(|s| !s.is_empty()) {
                Some(v) => v.to_string(),
                None => return Err($msg.to_string()),
            }
        };
    }

    let livekit_url_internal = require_field!(lk.url, "no LiveKit URL is configured");
    let api_key = require_field!(lk.api_key, "no LiveKit API key is configured");
    let api_secret = require_field!(lk.api_secret, "no LiveKit API secret is configured");

    let token = tddy_livekit::token::TokenGenerator::new(
        api_key,
        api_secret,
        livekit_room.to_string(),
        bridge_identity.to_string(),
        std::time::Duration::from_secs(tddy_livekit::token::DEFAULT_LIVEKIT_JWT_TTL_SECS),
    )
    .generate()
    .map_err(|e| format!("minting the bridge's LiveKit token: {e}"))?;

    Ok(BridgeSpawnConfig {
        host: target.host.clone(),
        port: target.port,
        username,
        password: String::new(),
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

/// How the question this host raises names the desktop it is about.
///
/// The label the operator gave it, and the endpoint it points at: a host may have several desktops,
/// and a dialog naming none of them asks somebody to type a secret into an unaddressed box. Neither
/// half is a secret — both are already in `ListHostTargets`.
fn desktop_prompt_subject(target: &HostDesktopTarget) -> String {
    format!("{} ({}:{})", target.label, target.host, target.port)
}

/// A desktop password for immediate use, from the ciphertext the operator's browser encrypted under
/// this host's published key.
///
/// The plaintext is owned by the caller and dies with the call that asked for it: `#hosts-screen
/// 6/8`'s prompt-decrypt-drop posture, deliberately not the session vault's storing one, so the
/// Hosts screen has a single secret-handling model. Nothing here writes it anywhere, and nothing
/// logs it — not even the reason a decrypt failed, which is the keypair's own words about a
/// ciphertext.
fn decrypt_desktop_password(
    keypair: &dyn HostKeypair,
    encrypted_password: &[u8],
) -> Result<String, Status> {
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

        let targets = scope
            .targets
            .list(&req.daemon_instance_id)
            .map_err(Status::internal)?;

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

        // Refused rather than wrapped: proto has no 16-bit integer, so a port past 65535 arrives
        // here intact and a cast would quietly attach a desktop to some other port entirely.
        let port = u16::try_from(req.port).map_err(|_| {
            Status::invalid_argument(format!(
                "{} is not a port a desktop can listen on",
                req.port
            ))
        })?;

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
                    port,
                    protocol: req.protocol,
                    username: req.username,
                },
            )
            .map_err(Status::internal)?;

        Ok(Response::new(AddHostTargetResponse { target_id }))
    }

    /// Starts a bridge for a host-scoped target, returning the same coordinates the session-scoped
    /// call does — the browser overlay already consumes exactly those fields.
    async fn start_host_stream(
        &self,
        request: Request<StartHostStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        let req = request.into_inner();
        let operator = self.require_user(&req.session_token)?;
        let scope = self.require_host_scope()?;
        let config = self.require_config()?;

        let target = scope
            .targets
            .list(&req.daemon_instance_id)
            .map_err(Status::internal)?
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

        // Settled before anybody is asked for a secret. A desktop this host cannot bridge — no
        // LiveKit to publish through, no protocol to speak — is refused here, so an operator is
        // never made to type a password into a dialog for a stream that could not have started.
        let prepared = prepare_bridge(
            config,
            &host_target_as_bridge_target(&target),
            target.username.clone(),
            &bridge_identity,
            &track_name,
            &livekit_room,
        )
        .map_err(Status::failed_precondition)?;

        // Asked for, not looked up: nothing on this host stores a desktop password, and the browser
        // could not have sent one — `HostPromptEvent` is the only place this host's public key is
        // published, so a caller has nothing to encrypt under until it has been asked. Read once,
        // handed to the bridge over its stdin, and dropped when this call returns.
        let password = self
            .prompt_for_desktop_password(scope, &operator, &target)
            .await?;

        // Reported, unlike the session path's deliberately silent spawn: these coordinates tell a
        // browser to mount an overlay on a room, and a bridge that never joined it leaves the
        // operator watching nothing having already typed a secret.
        self.spawn_bridge(
            prepared.for_password(password),
            host_bridge_key(&req.daemon_instance_id, &req.target_id),
        )
        .await
        .map_err(Status::internal)?;

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
    ) -> Result<Response<StopHostStreamResponse>, Status> {
        let req = request.into_inner();
        self.require_user(&req.session_token)?;
        // Checked here as it is on every other host-scoped call: a daemon that serves no host
        // desktops cannot have been holding one open, and `ok: true` for a stop it could not have
        // performed tells the browser to tear down an overlay over a bridge still running.
        self.require_host_scope()?;

        self.terminate_bridge(&host_bridge_key(&req.daemon_instance_id, &req.target_id))
            .await;

        Ok(Response::new(StopHostStreamResponse { ok: true }))
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
    use crate::host_prompts::{
        AnswerHandoff, AnswerRejection, InMemoryHostPromptRegistry, PendingPrompt, PromptKind,
    };
    use tddy_rpc::Code;

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
    const A_VNC_PORT: u32 = 5900;
    const A_DESKTOP_PASSWORD: &str = "correct horse battery staple";
    const A_VAULT_PASSPHRASE: &str = "hunter2-passphrase";
    /// What every desktop attached below is called. A prompt has to name the desktop it is asking
    /// about, so this is the word the dialog has to be able to show.
    const THE_DESKTOPS_LABEL: &str = "dev box";
    /// The password of a desktop that has none — what the operator sends back for the
    /// password-less desktops the rest of this module attaches.
    const NO_PASSWORD: &str = "";
    /// A session token this daemon's resolver answers nothing for — somebody who is not logged in.
    const A_TOKEN_THIS_DAEMON_DOES_NOT_KNOW: &str = "not-a-session-token";
    /// A path with no bridge binary at the end of it.
    const A_BRIDGE_BINARY_THAT_IS_NOT_INSTALLED: &str = "/nonexistent/tddy-vnc";
    /// One past the last port there is. Truncated to sixteen bits it becomes 1 — a port a machine
    /// really could be listening on, which is what makes the silent version of this so hard to see.
    const A_PORT_PAST_THE_LAST_ONE: u32 = 65_537;

    /// Safety nets, not predictions: a stub that has recorded nothing by now was never spawned, and
    /// a signalled process still alive by now was never signalled. Both cost nothing when the
    /// condition already holds.
    const A_BRIDGE_RECORDS_ITS_INVOCATION_WITHIN: Duration = Duration::from_secs(10);
    const A_SIGNALLED_BRIDGE_EXITS_WITHIN: Duration = Duration::from_secs(10);

    /// Longer than any test here runs, so a bridge only ever exits because it was released.
    const A_BRIDGE_STAYS_UP_FOR_SECS: u32 = 300;

    /// A safety net around a start whose question is never answered: long enough that no slow
    /// machine trips it, short enough that a start which waits for ever fails as a failure rather
    /// than as a hung suite.
    const A_START_NOBODY_ANSWERS_GIVES_UP_WITHIN: Duration = Duration::from_secs(15);

    /// Whether `haystack` contains `needle` anywhere in it — files the daemon wrote are bytes, not
    /// text, and a keypair or a vault is not UTF-8.
    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

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

    /// What the person in front of the browser does when this host asks for a desktop password.
    ///
    /// Named for the human rather than for the registry, because that is the variable: the
    /// registry underneath is the real [`InMemoryHostPromptRegistry`], so expiry, single use and
    /// the handoff behave exactly as they do in production. Only *who answers, and whether* is
    /// stood in for.
    #[derive(Debug, Clone, Copy)]
    enum TheOperator {
        /// Types this password and sends it, encrypted under the host's published key — the same
        /// RSA-OAEP(SHA-256) form the browser's `encryptForHost` produces.
        Types(&'static str),
        /// Walks away without answering.
        ///
        /// The question is stamped as already past its expiry, so "nobody answered in time" costs
        /// no sleep. That expiry is not what is being tested here — [`crate::host_prompts`] proves
        /// the registry reaps its own prompts — what is being tested is what the *start* does when
        /// the answer it is waiting for never comes.
        WalksAway,
    }

    /// The operator at the far end of node 6's prompt channel, and a record of everything this
    /// host asked them.
    struct AnOperatorAtTheKeyboard {
        registry: InMemoryHostPromptRegistry,
        asked: std::sync::Mutex<Vec<PendingPrompt>>,
        behaviour: TheOperator,
        /// Who this person is, fixed when they sat down — **not** whoever the prompt arrived
        /// stamped for.
        ///
        /// That distinction is the whole value of this fixture. Answering as `issued_for` would
        /// agree with production whatever identity it stamped, so the registry's ownership check
        /// would be checking a value against itself and a prompt stamped for a stranger would sail
        /// through. Answering as a fixed identity puts the real check in the way.
        answers_as: String,
        /// Needed to answer at all: an answer travels as ciphertext under this host's published
        /// key, which is the only form `StartHostStream` can accept.
        keypair: Arc<FileHostKeypair>,
    }

    impl AnOperatorAtTheKeyboard {
        fn new(behaviour: TheOperator, keypair: Arc<FileHostKeypair>, answers_as: &str) -> Self {
            Self {
                registry: InMemoryHostPromptRegistry::new(),
                asked: std::sync::Mutex::new(Vec::new()),
                behaviour,
                answers_as: answers_as.to_string(),
                keypair,
            }
        }

        /// Every question this host has raised, in the order it raised them.
        fn questions_asked(&self) -> Vec<PendingPrompt> {
            self.asked
                .lock()
                .expect("nothing panics holding this")
                .clone()
        }
    }

    impl HostPromptRegistry for AnOperatorAtTheKeyboard {
        fn issue(
            &self,
            issued_for: &str,
            kind: PromptKind,
            subject: &str,
            now_unix_ms: i64,
        ) -> PendingPrompt {
            let issued = self.registry.issue(issued_for, kind, subject, now_unix_ms);
            self.asked
                .lock()
                .expect("nothing panics holding this")
                .push(issued.clone());
            match self.behaviour {
                TheOperator::Types(password) => {
                    // Answered as the browser does: the plaintext is encrypted here and only the
                    // ciphertext is handed to the registry, so nothing in this fixture proves
                    // anything the real channel would not have to.
                    //
                    // Answered as **this** operator — never as `issued_for`. The registry accepts
                    // an answer only from the operator a prompt was raised for, so a start that
                    // stamped its question with anybody else is refused here by the real check
                    // rather than quietly answered by a fixture agreeing with itself.
                    let ciphertext = encrypted_under(&self.keypair, password);
                    self.registry
                        .answer(&issued.prompt_id, &self.answers_as, ciphertext, now_unix_ms)
                        .unwrap_or_else(|rejection| {
                            panic!(
                                "the operator at this keyboard is {:?}, and the question this host \
                                 raised was stamped for {issued_for:?}, so they cannot answer it: \
                                 {rejection:?}",
                                self.answers_as
                            )
                        });
                    issued
                }
                // Handed back already expired, so the waiter's own deadline has passed before it
                // starts waiting.
                TheOperator::WalksAway => PendingPrompt {
                    expires_at_unix_ms: now_unix_ms,
                    ..issued
                },
            }
        }

        fn pending(&self, issued_for: &str, now_unix_ms: i64) -> Vec<PendingPrompt> {
            self.registry.pending(issued_for, now_unix_ms)
        }

        fn answer(
            &self,
            prompt_id: &str,
            answered_by: &str,
            encrypted_answer: Vec<u8>,
            now_unix_ms: i64,
        ) -> Result<(), AnswerRejection> {
            self.registry
                .answer(prompt_id, answered_by, encrypted_answer, now_unix_ms)
        }

        fn awaited_answer(&self, prompt_id: &str) -> Option<AnswerHandoff> {
            self.registry.awaited_answer(prompt_id)
        }

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PendingPrompt> {
            self.registry.subscribe()
        }
    }

    /// A password as the browser sends it: RSA-OAEP(SHA-256) under this host's published key.
    fn encrypted_under(keypair: &FileHostKeypair, password: &str) -> Vec<u8> {
        use rsa::pkcs8::DecodePublicKey;

        let PublishedKey { spki_der, .. } = keypair.published().expect("this host's published key");
        rsa::RsaPublicKey::from_public_key_der(&spki_der)
            .expect("a published SPKI DER public key")
            .encrypt(
                &mut rand::thread_rng(),
                rsa::Oaep::new::<sha2::Sha256>(),
                password.as_bytes(),
            )
            .expect("a password fits comfortably in an OAEP payload")
    }

    /// The service wired the way `runtime.rs` wires it — real target store, real keypair, real
    /// vault — over throwaway directories, with the bridge binary pointed at [`FakeBridge`].
    ///
    /// Everything below the RPC is the production code path; only the desktop at the far end is
    /// stood in for, because a bridge cannot connect to a machine that is not there.
    struct DaemonUnderTest {
        service: ScreenSharingServiceImpl,
        bridge: FakeBridge,
        operator: Arc<AnOperatorAtTheKeyboard>,
        storage: tempfile::TempDir,
    }

    /// A daemon whose operator answers with the empty password.
    ///
    /// Every desktop these tests attach is password-less, so that is what a person in front of the
    /// browser would send. Stated rather than left absent: a start that raises a prompt still
    /// completes, and the tests about rooms, identities and stop keep measuring what they are
    /// about rather than turning into prompt tests.
    fn a_daemon() -> DaemonUnderTest {
        a_daemon_whose_operator(TheOperator::Types(NO_PASSWORD))
    }

    fn a_daemon_whose_operator(behaviour: TheOperator) -> DaemonUnderTest {
        a_daemon_configured(behaviour, |_as_wired_in_production| {})
    }

    /// A daemon that has a room to publish into but no credentials to mint a token with.
    ///
    /// Half-configured rather than unconfigured on purpose: `host_livekit_room` already refuses a
    /// daemon with no common room, so only a daemon that gets *past* that check reaches the
    /// preparation this is about.
    fn a_daemon_that_cannot_mint_a_livekit_token() -> DaemonUnderTest {
        a_daemon_configured(TheOperator::Types(NO_PASSWORD), |config| {
            config
                .livekit
                .as_mut()
                .expect("this daemon's LiveKit configuration")
                .api_secret = None;
        })
    }

    /// A daemon pointed at a bridge binary that is not installed.
    fn a_daemon_whose_bridge_binary_is_missing() -> DaemonUnderTest {
        a_daemon_configured(TheOperator::Types(NO_PASSWORD), |config| {
            config
                .screen_sharing
                .as_mut()
                .expect("this daemon's screen sharing configuration")
                .vnc_binary_path = A_BRIDGE_BINARY_THAT_IS_NOT_INSTALLED.to_string();
        })
    }

    /// A daemon that was never wired for host scope — `with_host_scope` uncalled, as on a daemon
    /// serving no Hosts screen at all.
    fn a_daemon_that_serves_no_host_desktops() -> DaemonUnderTest {
        let mut daemon = a_daemon();
        daemon.service.host_scope = None;
        daemon
    }

    fn a_daemon_configured(
        behaviour: TheOperator,
        adjust: impl FnOnce(&mut DaemonConfig),
    ) -> DaemonUnderTest {
        let storage = tempfile::tempdir().expect("a storage directory");
        let bridge = FakeBridge::installed_in(storage.path());

        let mut config = DaemonConfig {
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
        };
        adjust(&mut config);
        let config = Arc::new(config);

        let sessions_base = storage.path().to_path_buf();
        let keypair = Arc::new(FileHostKeypair::new(storage.path()));
        let targets = Arc::new(FileHostDesktopTargetStore::new(storage.path()));
        let operator = Arc::new(AnOperatorAtTheKeyboard::new(
            behaviour,
            Arc::clone(&keypair),
            THE_OS_USER,
        ));

        let service = ScreenSharingServiceImpl::new(
            Arc::new(|token: &str| (token == A_SESSION_TOKEN).then(|| THE_OS_USER.to_string())),
            Arc::new(move |_user: &str| Some(sessions_base.clone())),
            Arc::new(Mutex::new(HashMap::new())),
        )
        .with_config(config)
        .with_host_scope(
            targets,
            Arc::clone(&keypair) as Arc<dyn HostKeypair>,
            Arc::clone(&operator) as Arc<dyn HostPromptRegistry>,
        );

        DaemonUnderTest {
            service,
            bridge,
            operator,
            storage,
        }
    }

    impl DaemonUnderTest {
        /// Attach a VNC desktop to `host`, as the Hosts screen does before opening one.
        async fn attach_a_desktop_to(&self, host: &str) -> String {
            self.try_to_attach_a_desktop_to(host, A_VNC_PORT)
                .await
                .expect("attaching a desktop to a host")
        }

        /// Attach a desktop on `port` and hand back whatever the call came to, refusal included.
        async fn try_to_attach_a_desktop_to(
            &self,
            host: &str,
            port: u32,
        ) -> Result<String, Status> {
            self.service
                .add_host_target(Request::new(AddHostTargetRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    label: THE_DESKTOPS_LABEL.to_string(),
                    host: "127.0.0.1".to_string(),
                    port,
                    protocol: Protocol::Vnc as i32,
                    username: "ada".to_string(),
                }))
                .await
                .map(|attached| attached.into_inner().target_id)
        }

        async fn open_the_desktop_of(&self, host: &str, target_id: &str) -> StartStreamResponse {
            self.try_to_open_the_desktop_of(host, target_id)
                .await
                .expect("opening a host's desktop")
        }

        /// Open a desktop and hand back whatever the start came to, refusal included.
        ///
        /// Separate from [`Self::open_the_desktop_of`] rather than replacing it: a test about
        /// rooms and identities should read as "it opens", and only a test about a start that
        /// cannot finish has any business inspecting a `Status`.
        async fn try_to_open_the_desktop_of(
            &self,
            host: &str,
            target_id: &str,
        ) -> Result<StartStreamResponse, Status> {
            self.service
                .start_host_stream(Request::new(StartHostStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .map(Response::into_inner)
        }

        /// Every question this host raised on node 6's prompt channel.
        fn questions_this_host_asked(&self) -> Vec<PendingPrompt> {
            self.operator.questions_asked()
        }

        /// Everything the daemon itself has written under its storage directory.
        ///
        /// The stand-in bridge's own recordings are excluded by name — they exist because a test
        /// asked the bridge to write down what it was handed, and they are not a daemon writing a
        /// secret down. Everything else under this root *is* the daemon's: the host desktop target
        /// store, this host's keypair, and any session tree.
        fn files_the_daemon_wrote(&self) -> Vec<(PathBuf, Vec<u8>)> {
            let recorded_by_the_bridge = [
                self.bridge.binary.clone(),
                self.bridge.argv_file.clone(),
                self.bridge.stdin_file.clone(),
            ];
            let mut found = Vec::new();
            let mut to_walk = vec![self.storage.path().to_path_buf()];
            while let Some(dir) = to_walk.pop() {
                for entry in std::fs::read_dir(&dir).expect("reading the daemon's storage") {
                    let path = entry.expect("a directory entry").path();
                    if path.is_dir() {
                        to_walk.push(path);
                        continue;
                    }
                    if recorded_by_the_bridge.contains(&path) {
                        continue;
                    }
                    let bytes = std::fs::read(&path).expect("reading a file the daemon wrote");
                    found.push((path, bytes));
                }
            }
            found
        }

        async fn close_the_desktop_of(&self, host: &str, target_id: &str) {
            self.try_to_close_the_desktop_of(host, target_id)
                .await
                .expect("closing a host's desktop");
        }

        /// Close a desktop and hand back whatever the stop came to, refusal included.
        async fn try_to_close_the_desktop_of(
            &self,
            host: &str,
            target_id: &str,
        ) -> Result<(), Status> {
            self.service
                .stop_host_stream(Request::new(StopHostStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .map(|_| ())
        }

        /// The ids of the desktops attached to `host`, as the Hosts screen lists them.
        async fn the_desktops_of(&self, host: &str) -> Vec<String> {
            self.service
                .list_host_targets(Request::new(ListHostTargetsRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                }))
                .await
                .expect("listing a host's desktops")
                .into_inner()
                .targets
                .into_iter()
                .map(|t| t.id)
                .collect()
        }

        /// The ids of the desktops in the session's own vault.
        async fn the_desktops_of_the_session(&self) -> Vec<String> {
            self.service
                .list_targets(Request::new(ListTargetsRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    session_id: A_SESSION.to_string(),
                }))
                .await
                .expect("listing a session's desktops")
                .into_inner()
                .targets
                .into_iter()
                .map(|t| t.id)
                .collect()
        }

        // Every host-scoped call again, as somebody whose session token this daemon does not
        // know. Each hands back the refusal, because the refusal is the whole subject.

        async fn a_stranger_lists_the_desktops_of(&self, host: &str) -> Status {
            self.service
                .list_host_targets(Request::new(ListHostTargetsRequest {
                    session_token: A_TOKEN_THIS_DAEMON_DOES_NOT_KNOW.to_string(),
                    daemon_instance_id: host.to_string(),
                }))
                .await
                .expect_err("a stranger must not be shown a host's desktops")
        }

        async fn a_stranger_attaches_a_desktop_to(&self, host: &str) -> Status {
            self.service
                .add_host_target(Request::new(AddHostTargetRequest {
                    session_token: A_TOKEN_THIS_DAEMON_DOES_NOT_KNOW.to_string(),
                    daemon_instance_id: host.to_string(),
                    label: THE_DESKTOPS_LABEL.to_string(),
                    host: "127.0.0.1".to_string(),
                    port: 5900,
                    protocol: Protocol::Vnc as i32,
                    username: "ada".to_string(),
                }))
                .await
                .expect_err("a stranger must not attach a desktop to a host")
        }

        async fn a_stranger_opens_the_desktop_of(&self, host: &str, target_id: &str) -> Status {
            self.service
                .start_host_stream(Request::new(StartHostStreamRequest {
                    session_token: A_TOKEN_THIS_DAEMON_DOES_NOT_KNOW.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .expect_err("a stranger must not open a host's desktop")
        }

        async fn a_stranger_closes_the_desktop_of(&self, host: &str, target_id: &str) -> Status {
            self.service
                .stop_host_stream(Request::new(StopHostStreamRequest {
                    session_token: A_TOKEN_THIS_DAEMON_DOES_NOT_KNOW.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                }))
                .await
                .expect_err("a stranger must not close a host's desktop")
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
        // The key is written out here rather than asked of `host_bridge_key`, which would agree
        // with production whatever it built — including a key with the `host:` prefix dropped,
        // the one thing keeping a daemon instance id from colliding with a session id.
        let pid = daemon
            .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
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

    // ---------------------------------------------------------------------------------------
    // AC-7 — a desktop password is prompted through node 6's channel, and never persisted
    //
    // The prompt is **raised by the daemon**, not supplied by the caller, and it has to be: the
    // browser has no way to obtain this host's public key except from a prompt. `HostPromptEvent`
    // is the only place it is published, and it carries the fingerprint the client pins — so a
    // browser that has not been asked anything cannot encrypt anything.
    // ---------------------------------------------------------------------------------------

    /// AC-7. Nothing stores a host desktop password, so opening one has to ask for it — and the
    /// question has to say which desktop, or the operator is typing a secret into a dialog naming
    /// no machine.
    #[tokio::test]
    async fn opening_a_host_desktop_asks_the_operator_for_its_password() {
        // Given a desktop attached to a host, and an operator ready to answer
        let daemon = a_daemon_whose_operator(TheOperator::Types(A_DESKTOP_PASSWORD));
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened
        daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // Then this host raised exactly one question, on node 6's channel, for a desktop password
        let asked = daemon.questions_this_host_asked();
        assert_eq!(
            asked.len(),
            1,
            "opening a desktop asks its operator exactly one question, asked: {asked:?}"
        );
        assert_eq!(
            asked[0].kind,
            PromptKind::DesktopPassword,
            "a desktop password is its own kind of question; node 6's key passphrase is not it"
        );

        // …stamped for the operator who raised it, which is what makes it reachable at all:
        // `StreamHostPrompts` shows a prompt to nobody else, and nobody else can spend its answer
        assert_eq!(
            asked[0].issued_for, THE_OS_USER,
            "a question stamped for anybody else reaches no browser and can be answered by nobody"
        );

        // …naming the desktop it is about, which is all the dialog has to go on
        assert!(
            asked[0].subject.contains(THE_DESKTOPS_LABEL),
            "the question must name the desktop being opened, said: {:?}",
            asked[0].subject
        );
    }

    /// AC-7 + AC-10. What the operator typed has to arrive at the bridge, and arrive on the one
    /// channel that is not world-readable: argv is visible to every local account through `ps`.
    ///
    /// The stdin assertion comes first on purpose. The daemon passes the bridge **no arguments at
    /// all**, so "the password is not in argv" is true of a password that never travelled — and an
    /// assertion that holds for the wrong reason reports the property as verified forever.
    #[tokio::test]
    async fn the_password_the_operator_typed_reaches_the_bridge_on_stdin_and_never_its_arguments() {
        // Given a desktop whose operator answers with its password
        let daemon = a_daemon_whose_operator(TheOperator::Types(A_DESKTOP_PASSWORD));
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened
        daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // Then the bridge was handed the answered password on its stdin
        let config = daemon.bridge.config_read_from_its_stdin().await;
        assert_eq!(
            config["password"].as_str(),
            Some(A_DESKTOP_PASSWORD),
            "the answered password must reach the bridge, or nothing below means anything"
        );

        // …and it appears nowhere on the command line
        let argv = daemon.bridge.recorded_argv().await;
        assert!(
            !argv.iter().any(|arg| arg.contains(A_DESKTOP_PASSWORD)),
            "the desktop password reached the bridge's argv: {argv:?}"
        );
    }

    /// AC-7. The whole reason this node prompts instead of using the session vault: a host desktop
    /// password is used once and dropped. A daemon that wrote it down would have quietly built the
    /// credential store this node exists not to build.
    #[tokio::test]
    async fn the_password_the_operator_typed_is_never_written_to_disk() {
        // Given a desktop whose operator answers with its password
        let daemon = a_daemon_whose_operator(TheOperator::Types(A_DESKTOP_PASSWORD));
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened
        daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // Then the password really did pass through this daemon — a secret that never existed is
        // trivially absent from every file, and would make the rest of this test prove nothing
        let config = daemon.bridge.config_read_from_its_stdin().await;
        assert_eq!(
            config["password"].as_str(),
            Some(A_DESKTOP_PASSWORD),
            "the answered password must reach the bridge, or nothing below means anything"
        );

        // …and it is in none of the files this daemon keeps
        let written = daemon.files_the_daemon_wrote();
        assert!(
            written
                .iter()
                .any(|(path, _)| path.file_name() == Some("host-desktop-targets.json".as_ref())),
            "the scan must cover where this daemon actually writes; it found only {:?}",
            written.iter().map(|(path, _)| path).collect::<Vec<_>>()
        );
        for (path, bytes) in &written {
            assert!(
                !contains_bytes(bytes, A_DESKTOP_PASSWORD.as_bytes()),
                "the desktop password was written to {}",
                path.display()
            );
        }
    }

    /// AC-7. An operator who walks away must not leave the call parked forever, and must not leave
    /// a bridge running against a desktop it could not authenticate to.
    #[tokio::test]
    async fn a_desktop_password_nobody_answers_fails_the_start_rather_than_starting_without_it() {
        // Given a desktop whose operator never answers the question
        let daemon = a_daemon_whose_operator(TheOperator::WalksAway);
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When it is opened
        let outcome = tokio::time::timeout(
            A_START_NOBODY_ANSWERS_GIVES_UP_WITHIN,
            daemon.try_to_open_the_desktop_of(A_HOST, &target_id),
        )
        .await
        .expect("a start whose question nobody answers must give up, not wait for ever");

        // Then the start says the question went unanswered
        let refusal = outcome.expect_err(
            "a desktop that needs a password must not report a stream it never authenticated",
        );
        assert_eq!(
            refusal.code(),
            Code::DeadlineExceeded,
            "an unanswered question is a deadline, not an internal error: {refusal:?}"
        );

        // …and no bridge was left running against a desktop it has no password for
        assert!(
            daemon
                .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
                .await
                .is_none(),
            "a bridge was started without the password the start was waiting for"
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
        // …and it asked nobody anything: a session desktop's password is in its vault, and routing
        // this path through the host prompt too would put a dialog in front of an operator who
        // already unlocked it
        assert!(
            daemon.questions_this_host_asked().is_empty(),
            "opening a session desktop asked its operator a question it has no business asking: {:?}",
            daemon.questions_this_host_asked()
        );
    }

    // ---------------------------------------------------------------------------------------
    // A start that could not have started must say so
    //
    // Coordinates for a room no bridge ever joined are worse than a refusal: the browser mounts an
    // overlay and waits on a track that will never appear, with nothing to say why — and by then
    // the operator has already typed a secret into a dialog.
    // ---------------------------------------------------------------------------------------

    /// A desktop this host could never bridge is refused before the question is even raised. The
    /// password is the one part of this flow that costs a person something to supply, so it is the
    /// last thing to ask for, not the first.
    #[tokio::test]
    async fn a_desktop_this_host_cannot_bridge_is_refused_before_anybody_is_asked_for_its_password()
    {
        // Given a host with a room to publish into but no credentials to mint a token with
        let daemon = a_daemon_that_cannot_mint_a_livekit_token();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When its desktop is opened
        let outcome = daemon.try_to_open_the_desktop_of(A_HOST, &target_id).await;

        // Then the start is refused rather than reporting a stream nothing publishes into
        let refusal =
            outcome.expect_err("a desktop with nowhere to publish must not report a stream");
        assert_eq!(
            refusal.code(),
            Code::FailedPrecondition,
            "a host that cannot reach LiveKit is a precondition, not a bad request: {refusal:?}"
        );

        // …and nobody was made to type a password for a stream that could never have started
        let asked = daemon.questions_this_host_asked();
        assert!(
            asked.is_empty(),
            "the operator was asked for a secret before this host knew it could bridge: {asked:?}"
        );
    }

    /// A bridge that never spawned is not a stream. Reported as a failure, the browser can say the
    /// desktop did not open; reported as success it mounts an overlay on an empty room.
    #[tokio::test]
    async fn a_bridge_that_cannot_be_spawned_fails_the_start_rather_than_reporting_a_stream() {
        // Given a host pointed at a bridge binary that is not installed
        let daemon = a_daemon_whose_bridge_binary_is_missing();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When its desktop is opened
        let outcome = daemon.try_to_open_the_desktop_of(A_HOST, &target_id).await;

        // Then the start says the bridge did not start
        let refusal = outcome.expect_err("a bridge that never spawned is not a stream");
        assert_eq!(
            refusal.code(),
            Code::Internal,
            "a bridge binary that will not spawn is this host's own failure: {refusal:?}"
        );

        // …and nothing is left recorded for a desktop that never opened
        assert!(
            daemon
                .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
                .await
                .is_none(),
            "a bridge that never spawned must not be tracked as running"
        );
    }

    /// A second start for the same desktop must release the first bridge. `active_bridges` holds
    /// one pid per key, so a start that overwrote one would leave a process nothing can ever
    /// signal — streaming a machine nobody is watching until the host reboots.
    #[tokio::test]
    async fn reopening_a_host_desktop_releases_the_bridge_the_previous_start_left_running() {
        // Given a host desktop that has been opened once
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;
        daemon.open_the_desktop_of(A_HOST, &target_id).await;
        let the_first_bridge = daemon
            .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
            .await
            .expect("opening a desktop spawns a bridge and records its pid");

        // When the same desktop is opened again
        daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // Then the bridge the first start left behind is gone
        eventually(
            "the bridge of the previous start exits when a second start replaces it",
            A_SIGNALLED_BRIDGE_EXITS_WITHIN,
            || {
                if process_is_alive(the_first_bridge) {
                    Err(format!("bridge pid {the_first_bridge} is still running"))
                } else {
                    Ok(())
                }
            },
        )
        .await;

        // …and the desktop is still addressable, now by the bridge that replaced it
        let the_current_bridge = daemon
            .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
            .await
            .expect("the second start's bridge must be the one the key now names");
        assert_ne!(
            the_current_bridge, the_first_bridge,
            "the second start must have spawned a bridge of its own"
        );
        assert!(
            process_is_alive(the_current_bridge),
            "the desktop's current bridge must be running"
        );
    }

    // ---------------------------------------------------------------------------------------
    // AC-8 — host scope and session scope are separate stores
    // ---------------------------------------------------------------------------------------

    /// A desktop attached to a machine outlives every session on it, and a session's desktop
    /// belongs to the work rather than to the box. Each half states what its list *does* hold as
    /// well as what it does not: an absence on its own would hold just as well against a daemon
    /// that lists nothing at all.
    #[tokio::test]
    async fn a_hosts_desktops_and_a_sessions_desktops_stay_out_of_each_others_lists() {
        // Given one desktop in a session's unlocked vault, and one attached to this host
        let daemon = a_daemon();
        let the_sessions_desktop = daemon.a_session_holding_a_desktop().await;
        let the_hosts_desktop = daemon.attach_a_desktop_to(A_HOST).await;

        // When each scope is listed
        let on_the_host = daemon.the_desktops_of(A_HOST).await;
        let in_the_session = daemon.the_desktops_of_the_session().await;

        // Then each list holds its own desktop, and only its own
        assert_eq!(
            on_the_host,
            vec![the_hosts_desktop],
            "a host lists the desktops attached to it, and nothing out of a session's vault"
        );
        assert_eq!(
            in_the_session,
            vec![the_sessions_desktop],
            "a session lists the desktops in its vault, and nothing attached to the host"
        );
    }

    // ---------------------------------------------------------------------------------------
    // Every host-scoped call belongs to a logged-in operator
    //
    // A desktop is an address on somebody's machine and a bridge is a process on it. One test per
    // call, because a missing check is deleted one line at a time.
    // ---------------------------------------------------------------------------------------

    #[tokio::test]
    async fn listing_a_hosts_desktops_without_a_session_this_daemon_knows_is_refused() {
        // Given a host with a desktop attached
        let daemon = a_daemon();
        daemon.attach_a_desktop_to(A_HOST).await;

        // When somebody this daemon does not know asks for its desktops
        let refusal = daemon.a_stranger_lists_the_desktops_of(A_HOST).await;

        // Then they are told who they are not, rather than shown the machine's addresses
        assert_eq!(refusal.code(), Code::Unauthenticated, "{refusal:?}");
    }

    #[tokio::test]
    async fn attaching_a_desktop_without_a_session_this_daemon_knows_is_refused() {
        // Given a host
        let daemon = a_daemon();

        // When somebody this daemon does not know attaches a desktop to it
        let refusal = daemon.a_stranger_attaches_a_desktop_to(A_HOST).await;

        // Then nothing is attached on their say-so
        assert_eq!(refusal.code(), Code::Unauthenticated, "{refusal:?}");
        assert!(
            daemon.the_desktops_of(A_HOST).await.is_empty(),
            "a refused attach must not have attached anything"
        );
    }

    #[tokio::test]
    async fn opening_a_host_desktop_without_a_session_this_daemon_knows_is_refused() {
        // Given a host with a desktop attached
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;

        // When somebody this daemon does not know opens it
        let refusal = daemon
            .a_stranger_opens_the_desktop_of(A_HOST, &target_id)
            .await;

        // Then no bridge is started for them
        assert_eq!(refusal.code(), Code::Unauthenticated, "{refusal:?}");
        assert!(
            daemon
                .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
                .await
                .is_none(),
            "a refused start must not have spawned a bridge"
        );
    }

    #[tokio::test]
    async fn closing_a_host_desktop_without_a_session_this_daemon_knows_is_refused() {
        // Given an open host desktop
        let daemon = a_daemon();
        let target_id = daemon.attach_a_desktop_to(A_HOST).await;
        daemon.open_the_desktop_of(A_HOST, &target_id).await;

        // When somebody this daemon does not know closes it
        let refusal = daemon
            .a_stranger_closes_the_desktop_of(A_HOST, &target_id)
            .await;

        // Then the stop is refused, and the operator's bridge is left alone
        assert_eq!(refusal.code(), Code::Unauthenticated, "{refusal:?}");
        assert!(
            daemon
                .bridge_running_for(&format!("host:{A_HOST}:{target_id}"))
                .await
                .is_some(),
            "a refused stop must not have released somebody else's bridge"
        );
    }

    /// A stop is the one host-scoped call that could answer `ok: true` having done nothing at all.
    /// On a daemon that serves no host desktops there is no bridge it could have released, and
    /// saying otherwise tells the browser to tear down an overlay over a stream still running.
    #[tokio::test]
    async fn closing_a_host_desktop_on_a_daemon_that_serves_none_is_refused_rather_than_reported_ok(
    ) {
        // Given a daemon that was never wired for host-scoped desktops
        let daemon = a_daemon_that_serves_no_host_desktops();

        // When a desktop of a host is closed on it
        let outcome = daemon
            .try_to_close_the_desktop_of(A_HOST, "a-desktop-this-daemon-never-served")
            .await;

        // Then it says it serves none, rather than reporting a stop it could not have performed
        let refusal =
            outcome.expect_err("a daemon serving no host desktops cannot have closed one");
        assert_eq!(
            refusal.code(),
            Code::FailedPrecondition,
            "an unwired daemon is a precondition, the same one every other host call reports: {refusal:?}"
        );
    }

    /// Proto has no sixteen-bit integer, so a port past the last one arrives here intact. Cast, it
    /// wraps into a perfectly plausible port and the desktop is quietly attached to the wrong one —
    /// an address nobody typed, on a machine somebody else may well be listening on.
    #[tokio::test]
    async fn attaching_a_desktop_on_a_port_no_machine_can_listen_on_is_refused() {
        // Given a host
        let daemon = a_daemon();

        // When a desktop is attached on a port past the last one there is
        let outcome = daemon
            .try_to_attach_a_desktop_to(A_HOST, A_PORT_PAST_THE_LAST_ONE)
            .await;

        // Then the caller is told, rather than handed an id for an address they never gave
        let refusal = outcome.expect_err("a port past 65535 is not a port a desktop listens on");
        assert_eq!(
            refusal.code(),
            Code::InvalidArgument,
            "a port that cannot exist is a bad request, not this host's failure: {refusal:?}"
        );

        // …and nothing was attached to some other port instead
        assert!(
            daemon.the_desktops_of(A_HOST).await.is_empty(),
            "a refused attach must not have attached a desktop anywhere"
        );
    }
}
