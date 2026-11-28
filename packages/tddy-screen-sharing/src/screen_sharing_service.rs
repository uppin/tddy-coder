//! Screen sharing control-plane service for tddy-daemon.
//!
//! Implements `ScreenSharingService` from `screen_sharing.proto` over the HTTP Connect
//! transport, and spawns/terminates protocol bridge processes (tddy-vnc / tddy-rdp) on demand.
//!
//! A target's password is a `screen-sharing` record in the session-gated credential store, reached
//! through [`ScreenSharingTargetStore`]. **No RPC here carries a passphrase**: the caller's session
//! is what opens the store, so there is nothing to unlock and `UnlockVault` no longer exists.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::bridge_config::session_bridge_key;
use crate::host_desktop_sign_in::RememberedDesktopCredentials;
use crate::screen_sharing_records::{ScreenSharingTargetStore, TargetError};
use crate::session_vault_target_store::SessionVaultTargetStore;
use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::host_desktop_targets::HostDesktopTargetStore;
use tddy_host_service::host_keypair::HostKeypair;
use tddy_host_service::host_prompts::HostPromptRegistry;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddHostTargetResponse, AddTargetRequest, AddTargetResponse,
    InspectHostDesktopRequest, InspectHostDesktopResponse, ListHostTargetsRequest,
    ListHostTargetsResponse, ListTargetsRequest, ListTargetsResponse, Protocol,
    RemoveTargetRequest, RemoveTargetResponse, ScreenSharingService,
    ScreenSharingTarget as ProtoScreenSharingTarget, StartHostStreamRequest, StartStreamRequest,
    StartStreamResponse, StopHostStreamRequest, StopHostStreamResponse, StopStreamRequest,
    StopStreamResponse,
};

pub(crate) const DEFAULT_STREAM_FPS: u32 = 30;

type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
/// Per-OS-user sessions base resolver: `Arc<dyn Fn(&str) -> Option<PathBuf>>`.
///
/// Wired in `runtime.rs` with the daemon's resolved `tddy_data_dir` (config-only tddy home), so a
/// session's metadata is read from the same data root as the session trees.
pub type SessionsBase = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// A desktop a bridge can be spawned for, from either scope.
///
/// What the spawn path reads and nothing more: the label and username a scope stores travel
/// separately, so the two scopes meet at the fields a bridge is addressed and configured by.
pub(crate) struct BridgeTarget {
    pub(crate) id: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) protocol: Protocol,
    /// The one device on `host` the bridge streams (an iOS Simulator's udid); `None` otherwise.
    pub(crate) device: Option<String>,
}

impl BridgeTarget {
    /// A stored session target. Refused rather than coerced when its port is not one: proto has no
    /// 16-bit integer, and a cast would quietly connect to some other port entirely.
    fn from_proto(target: &ProtoScreenSharingTarget) -> Result<Self, String> {
        Ok(Self {
            id: target.id.clone(),
            host: target.host.clone(),
            port: u16::try_from(target.port)
                .map_err(|_| format!("{} is not a port a desktop can listen on", target.port))?,
            protocol: Protocol::try_from(target.protocol).unwrap_or(Protocol::Unspecified),
            device: Some(target.device_id.clone()).filter(|device| !device.is_empty()),
        })
    }
}

/// What the host-scoped calls need, and the session-scoped ones do not.
///
/// One value rather than two independent options: a daemon that can store a host's desktop but
/// cannot read the password prompted for it can start nothing, so "host scope is wired" is a
/// single fact.
// TODO(restructure): hand-widened to pub(crate) -- `prompt_for_desktop_password`, now in
// `bridge_lifecycle` at pub(crate), names this type (E private_interfaces); the engine widens moved
// items only, and this one did not move.
pub(crate) struct HostScope {
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
    /// Optional daemon config — when set, `start_stream` spawns real bridge processes.
    config: Option<Arc<DaemonConfig>>,
    /// Mints the room token a bridge joins with — see [`Self::with_room_tokens`].
    room_tokens: Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
    /// Active bridge PIDs keyed by [`session_bridge_key`] or [`host_bridge_key`], for the matching
    /// stop call.
    active_bridges: Arc<Mutex<HashMap<String, u32>>>,
    /// Absent until the daemon wires host scope; the host-scoped calls report that rather than
    /// pretending a host has no desktops.
    host_scope: Option<HostScope>,
    /// The credential store this session's targets live in.
    ///
    /// Optional for the same reason [`config`](Self::config) is: the wiring layer supplies it, and
    /// a service built without one reports that rather than answering as though the person has no
    /// desktops.
    target_store: Option<Arc<dyn ScreenSharingTargetStore>>,
    /// How long a start waits for its bridge to report the desktop ready.
    bridge_ready_timeout: std::time::Duration,
    /// The x11vnc serving each open X11 channel.
    x11vnc: Arc<crate::x11vnc::X11vncServers>,
    /// The host-desktop credentials operators asked to be remembered, in the same vaults the
    /// session targets live in. Absent when no vaults are wired: nothing is then remembered, and
    /// an answer asking to remember is refused rather than quietly forgotten.
    remembered: Option<RememberedDesktopCredentials>,
}

impl HostScope {
    /// The store a host's desktops live in.
    pub(crate) fn targets(&self) -> &dyn HostDesktopTargetStore {
        self.targets.as_ref()
    }
}

impl ScreenSharingServiceImpl {
    pub fn new(user_resolver: UserResolver, sessions_base: SessionsBase) -> Self {
        Self {
            user_resolver,
            sessions_base,
            config: None,
            room_tokens: Arc::new(tddy_daemon_extensions::NoRoomTokens),
            active_bridges: Arc::new(Mutex::new(HashMap::new())),
            host_scope: None,
            target_store: None,
            bridge_ready_timeout: crate::bridge_readiness::DEFAULT_BRIDGE_READY_TIMEOUT,
            x11vnc: Arc::new(crate::x11vnc::X11vncServers::default()),
            remembered: None,
        }
    }

    /// How long a start waits for its bridge's readiness line before failing the start and
    /// stopping the bridge.
    #[must_use]
    pub fn with_bridge_ready_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.bridge_ready_timeout = timeout;
        self
    }

    /// How long opening an X11 channel waits for its x11vnc to accept before failing the open.
    #[must_use]
    pub fn with_x11vnc_accept_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.x11vnc = Arc::new(crate::x11vnc::X11vncServers::new(timeout));
        self
    }

    /// Supply the credential store a session's screen-sharing targets are records in.
    #[must_use]
    pub fn with_target_store(mut self, target_store: Arc<dyn ScreenSharingTargetStore>) -> Self {
        self.target_store = Some(target_store);
        self
    }

    /// Keep targets in the user's credential `vaults`; with none, the targets calls refuse.
    #[must_use]
    pub fn with_credential_vaults(self, vaults: Option<Arc<SessionVaults>>) -> Self {
        let Some(vaults) = vaults else { return self };
        let resolver = Arc::clone(&self.user_resolver);
        let mut service = self.with_target_store(Arc::new(SessionVaultTargetStore::new(
            Arc::clone(&vaults),
            resolver,
        )));
        service.remembered = Some(RememberedDesktopCredentials::new(vaults));
        service
    }

    /// The host-desktop credentials operators remembered, when vaults are wired.
    pub(crate) fn remembered_desktop_credentials(&self) -> Option<&RememberedDesktopCredentials> {
        self.remembered.as_ref()
    }

    /// Mint bridges' room tokens through `minter` (builder) — the daemon's one minter, built from
    /// its `livekit:` block.
    pub fn with_room_tokens(
        mut self,
        minter: Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
    ) -> Self {
        self.room_tokens = minter;
        self
    }

    /// Supply daemon config for bridge binary path resolution and the LiveKit URL.
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
    pub(crate) fn require_user(&self, session_token: &str) -> Result<String, Status> {
        (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid session token"))
    }

    pub(crate) fn require_host_scope(&self) -> Result<&HostScope, Status> {
        self.host_scope.as_ref().ok_or_else(|| {
            Status::failed_precondition("this daemon serves no host-scoped desktop targets")
        })
    }

    pub(crate) fn require_config(&self) -> Result<&DaemonConfig, Status> {
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
}

mod session_stream;

mod bridge_lifecycle;
mod host_targets;

/// How a store refusal reaches the caller.
///
/// `ListTargets` never passes [`TargetError::Locked`] here: for it, locked is a *state* carried in
/// `vault_locked`, and turning it into a `Status` would collapse the distinction this node exists
/// to keep. Every other call has no such field, so for them a locked store is a refusal.
pub(crate) fn target_error_to_status(error: TargetError) -> Status {
    match error {
        TargetError::NoSuchSession => Status::unauthenticated("invalid session token"),
        TargetError::Locked => Status::failed_precondition(
            "the credential store is sealed under a different login and must be re-linked",
        ),
        TargetError::Malformed(reason) => {
            Status::internal(format!("a stored target is unusable: {}", reason))
        }
        TargetError::Unavailable(reason) => {
            Status::internal(format!("the credential store is unavailable: {}", reason))
        }
    }
}

#[async_trait]
impl ScreenSharingService for ScreenSharingServiceImpl {
    // --- Host-scoped targets (#hosts-screen 8/8) ---
    //
    // A desktop belongs to a machine, not to a coding session. These mirror the session-scoped
    // calls above, addressed by `daemon_instance_id`, and reuse the same bridge spawn path — host
    // scope is an addressing and storage change, not a second implementation.

    // TODO(restructure): hand-moved — extract_method refused under #[async_trait]; see docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md
    async fn list_host_targets(
        &self,
        request: Request<ListHostTargetsRequest>,
    ) -> Result<Response<ListHostTargetsResponse>, Status> {
        self.list_host_targets_impl(request).await
    }

    // TODO(restructure): hand-moved — extract_method refused under #[async_trait]; see docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md
    async fn add_host_target(
        &self,
        request: Request<AddHostTargetRequest>,
    ) -> Result<Response<AddHostTargetResponse>, Status> {
        self.add_host_target_impl(request).await
    }

    // TODO(restructure): hand-moved — extract_method refused under #[async_trait]; see docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md
    async fn start_host_stream(
        &self,
        request: Request<StartHostStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        self.start_host_stream_impl(request).await
    }

    // TODO(restructure): hand-moved — extract_method refused under #[async_trait]; see docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md
    async fn stop_host_stream(
        &self,
        request: Request<StopHostStreamRequest>,
    ) -> Result<Response<StopHostStreamResponse>, Status> {
        self.stop_host_stream_impl(request).await
    }

    async fn inspect_host_desktop(
        &self,
        request: Request<InspectHostDesktopRequest>,
    ) -> Result<Response<InspectHostDesktopResponse>, Status> {
        crate::host_desktop_sign_in::inspect_host_desktop(self, request).await
    }

    async fn list_targets(
        &self,
        request: Request<ListTargetsRequest>,
    ) -> Result<Response<ListTargetsResponse>, Status> {
        let req = request.into_inner();
        let store = self.require_target_store()?;

        // A locked store is a *state* of this response, not an error on it. The three outcomes a
        // screen must tell apart are open-and-empty, locked, and unavailable — collapsing the
        // middle one into the first is what makes a person re-add desktops they already have.
        match store.list(&req.session_token) {
            Ok(targets) => Ok(Response::new(ListTargetsResponse {
                targets,
                vault_locked: false,
            })),
            Err(TargetError::Locked) => Ok(Response::new(ListTargetsResponse {
                targets: vec![],
                vault_locked: true,
            })),
            Err(error) => Err(target_error_to_status(error)),
        }
    }

    async fn add_target(
        &self,
        request: Request<AddTargetRequest>,
    ) -> Result<Response<AddTargetResponse>, Status> {
        let req = request.into_inner();
        let store = self.require_target_store()?;

        // The id is the store's to mint — a target's id is its account at the `screen-sharing`
        // provider, so it is decided where the record is written.
        let requested = ProtoScreenSharingTarget {
            id: String::new(),
            label: req.label,
            host: req.host,
            port: req.port,
            protocol: req.protocol,
            username: req.username,
            x11_display: None,
            domain: String::new(),
            device_id: String::new(),
        };

        let target = store
            .add(&req.session_token, &requested, &req.password)
            .map_err(target_error_to_status)?;

        Ok(Response::new(AddTargetResponse {
            target: Some(target),
        }))
    }

    async fn remove_target(
        &self,
        request: Request<RemoveTargetRequest>,
    ) -> Result<Response<RemoveTargetResponse>, Status> {
        let req = request.into_inner();
        let store = self.require_target_store()?;

        store
            .remove(&req.session_token, &req.target_id)
            .map_err(target_error_to_status)?;

        Ok(Response::new(RemoveTargetResponse { ok: true }))
    }

    async fn start_stream(
        &self,
        request: Request<StartStreamRequest>,
    ) -> Result<Response<StartStreamResponse>, Status> {
        self.start_stream_impl(request).await
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
#[path = "screen_sharing_service_tests.rs"]
mod tests;
