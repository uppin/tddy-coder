use crate::screen_sharing_service::BridgeTarget;
use crate::screen_sharing_service::DEFAULT_STREAM_FPS;
use std::time::Duration;
use tddy_daemon_kernel::config::{resolve_rdp_binary_path, resolve_vnc_binary_path, DaemonConfig};
use tddy_host_service::host_desktop_targets::HostDesktopTarget;
use tddy_host_service::host_keypair::HostKeypair;
use tddy_rpc::Status;
use tddy_service::proto::screen_sharing::{
    Protocol, ScreenSharingTarget as ProtoScreenSharingTarget,
};

/// A bridge spawn that is ready except for the password nobody has typed yet.
///
/// Separated from the spawn so that everything knowable *without* a secret — the LiveKit
/// coordinates, the minted token, which binary speaks this desktop's protocol — is settled before
/// an operator is asked for one. A desktop this host could never bridge is then refused without
/// anybody typing a password for a stream that was never going to start.
pub(crate) struct PreparedBridge {
    pub(crate) binary: String,
    pub(crate) spawn_config: BridgeSpawnConfig,
}

impl PreparedBridge {
    pub(crate) fn for_password(mut self, password: String) -> Self {
        self.spawn_config.password = password;
        self
    }
}

/// Everything a bridge spawn needs except the desktop's password.
pub(crate) fn prepare_bridge(
    config: &DaemonConfig,
    target: &BridgeTarget,
    username: String,
    bridge_identity: &str,
    track_name: &str,
    livekit_room: &str,
    room_tokens: &dyn tddy_daemon_extensions::RoomTokenMinter,
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
            room_tokens,
        )?,
    })
}

/// How long a bridge's room token is valid: six hours, the length of a LiveKit access token's own
/// default, so a published desktop is not cut off by token expiry under normal use (the bridge has
/// no reconnect loop that fetches another).
const BRIDGE_TOKEN_TTL: Duration = Duration::from_secs(6 * 3600);

/// Extract the LiveKit URL, mint a token, and assemble a `BridgeSpawnConfig`.
///
/// Says what is missing rather than logging it: a host desktop that cannot be published anywhere
/// has to be refused, and the caller is the only place that knows whether that refusal reaches an
/// operator or only the log.
///
/// The password is left empty here and filled in by [`PreparedBridge::for_password`] — nothing in
/// this function needs one, which is exactly why it can run before anybody is asked.
pub(crate) fn build_bridge_spawn_config(
    config: &DaemonConfig,
    target: &BridgeTarget,
    username: String,
    bridge_identity: &str,
    track_name: &str,
    livekit_room: &str,
    room_tokens: &dyn tddy_daemon_extensions::RoomTokenMinter,
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

    let token = room_tokens
        .mint(livekit_room, bridge_identity, BRIDGE_TOKEN_TTL)
        .map_err(|refused| refused.message)?;

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
        // No size is hinted: the desktop's size is known only once the bridge has connected, and it
        // reports that size back on its readiness line.
        width: 0,
        height: 0,
        target_id: target.id.clone(),
        fps: DEFAULT_STREAM_FPS,
    })
}

/// How a session's running bridge is addressed in `active_bridges`.
pub(crate) fn session_bridge_key(session_id: &str, target_id: &str) -> String {
    format!("{}:{}", session_id, target_id)
}

/// How a host's running bridge is addressed in `active_bridges`.
///
/// Prefixed, so a daemon instance id can never collide with a session id and stop the wrong
/// bridge — the two id spaces are unrelated and neither is reserved against the other.
pub(crate) fn host_bridge_key(daemon_instance_id: &str, target_id: &str) -> String {
    format!("host:{}:{}", daemon_instance_id, target_id)
}

/// The identity the host's bridge joins LiveKit under, and the one the overlay subscribes to.
///
/// Carries the host id because every host's bridge publishes into the *same* common room; without
/// it, two hosts streaming at once would collide on one identity.
pub(crate) fn host_bridge_identity(daemon_instance_id: &str, target_id: &str) -> String {
    format!(
        "{}{}-{}",
        tddy_service::SCREEN_SHARE_HOST_IDENTITY_PREFIX,
        daemon_instance_id,
        target_id
    )
}

pub(crate) fn screenshare_track_name(target_id: &str) -> String {
    format!("screenshare:{}", target_id)
}

/// The LiveKit URL to hand the **browser** — `public_url` when the operator set one, since the
/// internal URL may not resolve outside the daemon's network.
pub(crate) fn browser_livekit_url(config: &DaemonConfig) -> String {
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
pub(crate) fn host_livekit_room(config: &DaemonConfig) -> Result<String, Status> {
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
pub(crate) fn desktop_prompt_subject(target: &HostDesktopTarget) -> String {
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
pub(crate) fn decrypt_desktop_password(
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
pub(crate) fn host_target_as_bridge_target(target: &HostDesktopTarget) -> BridgeTarget {
    BridgeTarget {
        id: target.target_id.clone(),
        host: target.host.clone(),
        port: target.port,
        protocol: Protocol::try_from(target.protocol).unwrap_or(Protocol::Unspecified),
    }
}

pub(crate) fn host_target_to_proto(t: &HostDesktopTarget) -> ProtoScreenSharingTarget {
    ProtoScreenSharingTarget {
        id: t.target_id.clone(),
        label: t.label.clone(),
        host: t.host.clone(),
        port: t.port as u32,
        protocol: t.protocol,
        username: t.username.clone(),
        x11_display: t.display,
        domain: String::new(),
    }
}

/// Bridge config serialized to the bridge binary's stdin (field names match `tddy_screenshare::BridgeConfig`).
#[derive(serde::Serialize)]
pub(crate) struct BridgeSpawnConfig {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) livekit_url: String,
    pub(crate) livekit_token: String,
    pub(crate) livekit_room: String,
    pub(crate) livekit_identity: String,
    pub(crate) track_name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) target_id: String,
    pub(crate) fps: u32,
}
