//! What `build` settles before it mounts a service: the data directory, the host registry, the
//! signing identity, the room-token minter and the auth entries every other service is gated by.

use super::*;
use tddy_daemon_auth::{AuthBuildResult, DaemonSigningKey, SessionTokens, SessionVaults};
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_livekit::common_room_supervisor::CommonRoomTarget;
use tddy_daemon_livekit::livekit_peer_discovery::{AdvertisedSigningKey, CommonRoomPeerRegistry};

// TODO(restructure-engine): the decomposition of `runtime::build` (D1) is a hand edit around
// `BuildContext`; every body below is a verbatim slice of the old function.

/// The inputs everything else is derived from.
pub(super) struct Keys {
    pub(super) tddy_data_dir: PathBuf,
    pub(super) auth_config: DaemonConfig,
    pub(super) host_registry: Arc<dyn tddy_session_lifecycle::host_registry::HostRegistry>,
    pub(super) common_room_peers: Option<(CommonRoomTarget, Arc<CommonRoomPeerRegistry>)>,
    pub(super) signing_key: Option<DaemonSigningKey>,
    pub(super) room_tokens: Arc<dyn tddy_daemon_extensions::RoomTokenMinter>,
}

/// [`Keys`] and what authenticating with them produced.
pub(super) struct Identity {
    pub(super) keys: Keys,
    pub(super) user_resolver: Option<SessionUserResolver>,
    pub(super) session_tokens: Option<SessionTokens>,
    pub(super) credential_vaults: Option<Arc<SessionVaults>>,
    pub(super) advertised_signing_key: Option<AdvertisedSigningKey>,
    pub(super) rpc_entries: Vec<tddy_rpc::ServiceEntry>,
}

impl Identity {
    pub(super) fn assemble(
        config: &DaemonConfig,
        options: &RuntimeOptions,
    ) -> anyhow::Result<Self> {
        let web = web_address(config)?;
        let keys = Keys::resolve(config, options)?;
        let auth_result = keys.auth_entries(config, options, web)?;
        let room_tokens = &keys.room_tokens;
        let signing_key = &keys.signing_key;
        // What the common room is told about this daemon's key, so a peer holding one of its tokens
        // can find the key that verifies it.
        let advertised_signing_key = signing_key
            .as_ref()
            .map(crate::common_room_key_directory::advertised_signing_key);
        let mut rpc_entries = auth_result.entries;

        // The room-JWT mint the web UI joins rooms through. Gated on the same session token as every
        // other daemon RPC — anything that can reach `/rpc` can call it, and a JWT it minted is
        // admission to a LiveKit room. See `tddy_session_lifecycle::auth::build_token_service_entry`.
        if let Some(entry) = tddy_session_lifecycle::auth::build_token_service_entry(
            config,
            auth_result.user_resolver.as_ref(),
            Arc::clone(room_tokens),
        ) {
            rpc_entries.push(entry);
        }
        Ok(Self {
            keys,
            user_resolver: auth_result.user_resolver,
            session_tokens: auth_result.session_tokens,
            credential_vaults: auth_result.credential_vaults,
            advertised_signing_key,
            rpc_entries,
        })
    }
}

/// The host and port the OAuth redirect URI is synthesised from.
fn web_address(config: &DaemonConfig) -> anyhow::Result<(String, u16)> {
    let web_host = config
        .listen
        .web_host
        .clone()
        .unwrap_or_else(|| "0.0.0.0".to_string());
    // Only used to synthesise the OAuth redirect URI a login comes back to. Absent, no login could
    // complete, so there is nothing to assemble.
    let web_port = config
        .listen
        .web_port
        .ok_or_else(|| anyhow::anyhow!("config.listen.web_port is required"))?;
    Ok((web_host, web_port))
}

fn auth_config_for(
    config: &DaemonConfig,
    options: &RuntimeOptions,
    tddy_data_dir: &std::path::Path,
) -> DaemonConfig {
    // The override reaches the auth services and stops there: `config` is what the settings service
    // persists, and a host-chosen callback address written into an operator's file would be a value
    // they never set — and would send a browser sign-in to loopback on some other machine.
    let mut auth_config = match &options.oauth_redirect_uri {
        Some(redirect_uri) => {
            let mut overridden = config.clone();
            if let Some(github) = overridden.github.as_mut() {
                github.redirect_uri = Some(redirect_uri.clone());
            }
            overridden
        }
        None => config.clone(),
    };
    // The data directory the rest of this runtime resolved — `TDDY_DATA_DIR` included — so that
    // with no `auth_storage` the signing key lives under the same directory as everything else
    // this daemon keeps, rather than under a second reading of the rule.
    auth_config.tddy_data_dir = Some(tddy_data_dir.to_path_buf());
    auth_config
}

impl Keys {
    fn resolve(config: &DaemonConfig, options: &RuntimeOptions) -> anyhow::Result<Self> {
        let tddy_data_dir = tddy_data_dir_for(config);
        let auth_config = auth_config_for(config, options, &tddy_data_dir);
        // One registry of durable host sightings for the whole daemon: peer discovery writes into it
        // and the `ListKnownHosts` handler reads it back. Two instances over the same directory would
        // each hold their own write lock, so a sighting and an RPC-time read could disagree.
        let host_registry: Arc<dyn tddy_session_lifecycle::host_registry::HostRegistry> = Arc::new(
            tddy_session_lifecycle::host_registry::FileHostRegistry::new(
                tddy_session_lifecycle::host_registry::host_registry_dir(&tddy_data_dir),
            ),
        );
        // The common room this daemon joins, if any, and its roster. Built before auth because the
        // roster is also where peers' session-token keys are read from: a token a peer minted is
        // verified against the key that peer advertises here. Resolved once, so the key directory and
        // peer discovery below cannot disagree about whether there is a room.
        let common_room_peers =
            CommonRoomTarget::from_livekit(config.livekit.as_ref()).map(|target| {
                let registry = Arc::new(
                    tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry::new()
                        .with_host_registry(Arc::clone(&host_registry)),
                );
                (target, registry)
            });
        // This daemon's signing identity, loaded — or generated on first boot — only when it has
        // someone to authenticate: with no `github:` block nothing here signs or verifies a token.
        // `mut`: `#keyring` 6/9's sync engine below takes ownership of it with `.take()`, the last use.
        let signing_key = match &auth_config.github {
            Some(_) => Some(tddy_daemon_auth::load_signing_key(&auth_config)?),
            None => None,
        };
        // Decided once, here, from the `livekit:` block: everything that mints a room token — the two
        // auth mints and screen sharing's bridges — asks this one minter.
        let room_tokens =
            tddy_daemon_livekit::room_tokens::room_token_minter(config.livekit.as_ref());
        Ok(Self {
            tddy_data_dir,
            auth_config,
            host_registry,
            common_room_peers,
            signing_key,
            room_tokens,
        })
    }

    fn auth_entries(
        &self,
        config: &DaemonConfig,
        options: &RuntimeOptions,
        web: (String, u16),
    ) -> anyhow::Result<AuthBuildResult> {
        let (web_host, web_port) = web;
        let auth_config = &self.auth_config;
        let signing_key = &self.signing_key;
        let common_room_peers = &self.common_room_peers;
        let room_tokens = &self.room_tokens;
        let auth_result = match &signing_key {
            Some(key) => {
                let directory: Arc<dyn tddy_daemon_auth::KeyDirectory> = match &common_room_peers {
                    Some((_, registry)) => Arc::new(
                        crate::common_room_key_directory::CommonRoomKeyDirectory::new(Arc::clone(
                            registry,
                        )),
                    ),
                    None => Arc::new(tddy_daemon_auth::StandaloneKeyDirectory),
                };
                log::info!(
                    target: tddy_daemon_auth::AUTH_LOG_TARGET,
                    "signing session tokens as key {}",
                    key.key_id()
                );
                tddy_daemon_auth::build_auth_entries_admitting(
                    auth_config,
                    (web_host.as_str(), web_port),
                    &tddy_daemon_auth::SessionTokens::new(key, directory),
                    first_login_enrolment(config, options)?,
                    Arc::clone(room_tokens),
                )?
            }
            None => tddy_session_lifecycle::auth::build_auth_entries(
                auth_config,
                web_host.as_str(),
                web_port,
                Arc::clone(room_tokens),
            )?,
        };
        Ok(auth_result)
    }
}
