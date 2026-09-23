//! Acceptance: a daemon believes a signing key only from another daemon.
//!
//! Product contract: `docs/ft/daemon/session-auth.md` and
//! `docs/ft/daemon/livekit-peer-discovery.md` § Trust model.
//!
//! A daemon verifies a peer's session token against the key that peer advertises on the common
//! room. The advertisement is self-declared metadata, so what makes it trustworthy is *who* may
//! publish one that discovery reads: only a participant whose identity no client-facing mint hands
//! out. This suite drives the attack end to end over a real LiveKit room — an authenticated web
//! user mints itself the only identity `token.TokenService` allows, joins the common room, advertises
//! a keypair of its own and signs a token for another login — and proves the fleet refuses it,
//! while a genuine peer daemon's token, advertised the same way, is accepted.
//!
//! Needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`); `#[serial]` so it owns
//! the container alone.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::RoomOptions;
use livekit::Room;
use serial_test::serial;
use tddy_daemon::common_room_key_directory::{advertised_signing_key, CommonRoomKeyDirectory};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon_auth::token_provider::LiveKitTokenProvider;
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, SIGNING_KEY_FILE};
use tddy_daemon_livekit::livekit_peer_discovery::{
    daemon_metadata_json, local_instance_id_for_config, parse_peer_daemon_json,
    spawn_common_room_discovery_loop, CommonRoomPeerRegistry, DaemonAdvertisement,
};
use tddy_github::session_token_v2::{SessionClaims, SessionTokenError};
use tddy_github::GitHubUser;
use tddy_livekit::TokenGenerator;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{Code, Request};
use tddy_service::proto::token::{GenerateTokenRequest, TokenService as _};
use tddy_service::TokenServiceImpl;
use tddy_testing_commons::wait::eventually;

const LK_API_KEY: &str = "devkey";
const LK_API_SECRET: &str = "secret";
const VERIFIER_INSTANCE_ID: &str = "key-trust-verifier";
const PEER_INSTANCE_ID: &str = "key-trust-peer";
/// The bare id the impostor would like to be — the shape of a daemon's discovery identity.
const IMPOSTOR_DAEMON_ID: &str = "key-trust-impostor";
/// The identity `token.TokenService` does hand it.
const IMPOSTOR_CLIENT_IDENTITY: &str = "web-key-trust-impostor";
/// A daemon-shaped participant re-advertising another daemon's key id.
const SHADOW_INSTANCE_ID: &str = "key-trust-shadow";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test]
#[serial]
async fn a_non_daemon_participant_advertising_a_signing_key_does_not_get_its_tokens_accepted() {
    // Given a daemon verifying tokens against the keys its common room advertises, and a genuine
    // peer daemon advertising its own
    let fleet = Fleet::start().await;
    let verifier = fleet.a_daemon(VERIFIER_INSTANCE_ID);
    let peer = fleet.a_daemon(PEER_INSTANCE_ID);

    // And an authenticated web user, refused the daemon-shaped identity it asks for first and
    // minted the browser identity it asks for next
    let mint = fleet.the_client_facing_mint();
    let refused = mint_room_token(&mint, &fleet.room, IMPOSTOR_DAEMON_ID).await;
    let impostor_jwt = mint_room_token(&mint, &fleet.room, IMPOSTOR_CLIENT_IDENTITY)
        .await
        .expect("a browser identity is minted");

    // And that user in the common room, advertising a keypair of its own exactly as a daemon does
    let impostor = Keypair::generate();
    let advertisement = impostor.advertisement_as(IMPOSTOR_DAEMON_ID);
    assert_eq!(
        parse_peer_daemon_json(&advertisement)
            .expect("the impostor's metadata is a well-formed advertisement")
            .advertisement
            .signing_key_id,
        impostor.key.key_id().to_string(),
        "the impostor must be advertising its key, or this test proves nothing"
    );
    let _impostor_room = join_advertising(&fleet.ws_url, &impostor_jwt, &advertisement).await;

    // When the verifying daemon has seen both — the peer discovered, the impostor's advertisement
    // on its roster — and each signs a token for the same login
    verifier.discovers(&peer.instance_id).await;
    verifier
        .sees_metadata_of(IMPOSTOR_CLIENT_IDENTITY, &advertisement)
        .await;
    let forged = impostor.key.signer().mint_access(&the_operator());
    let genuine = peer.key.signer().mint_access(&the_operator());

    // Then the daemon-shaped identity was never minted
    assert_eq!(
        refused.map_err(|status| status.code),
        Err(Code::PermissionDenied),
        "no client may be minted an identity discovery reads a key from"
    );
    // And the impostor's token names a key the verifier never learned, while the peer's verifies
    assert_eq!(
        (
            verifier.verifies(&forged).map(|claims| claims.login),
            verifier.verifies(&genuine).map(|claims| claims.login),
        ),
        (
            Err(SessionTokenError::UnknownKeyId(impostor.key.key_id())),
            Ok(the_operator().login),
        )
    );
}

#[tokio::test]
#[serial]
async fn a_participant_re_advertising_a_genuine_key_id_with_other_bytes_does_not_shadow_it() {
    // Given a verifying daemon, a genuine peer, and a second daemon-shaped participant advertising
    // the peer's key id with a key of its own — the one kind of participant discovery does read,
    // so only a holder of the LiveKit API secret could put it there
    let fleet = Fleet::start().await;
    let verifier = fleet.a_daemon(VERIFIER_INSTANCE_ID);
    let peer = fleet.a_daemon(PEER_INSTANCE_ID);
    let shadow = Keypair::generate();
    let shadowing = shadow.advertisement_claiming(SHADOW_INSTANCE_ID, &peer.key);
    let shadow_jwt = fleet
        .livekit
        .generate_token(&fleet.room, SHADOW_INSTANCE_ID)
        .expect("a room token for the shadow");
    let _shadow_room = join_advertising(&fleet.ws_url, &shadow_jwt, &shadowing).await;

    // When the verifier has both on its roster and is handed the genuine peer's token
    verifier.discovers(&peer.instance_id).await;
    verifier.discovers(SHADOW_INSTANCE_ID).await;
    let genuine = peer.key.signer().mint_access(&the_operator());

    // Then it verifies — the key that hashes to the id wins, whichever row the roster yields first
    assert_eq!(
        verifier.verifies(&genuine).map(|claims| claims.login),
        Ok(the_operator().login)
    );
}

#[tokio::test]
#[serial]
async fn a_peers_key_once_learned_still_verifies_while_the_roster_is_empty() {
    // Given a verifying daemon that has learned a peer's key by verifying one of its tokens
    let fleet = Fleet::start().await;
    let verifier = fleet.a_daemon(VERIFIER_INSTANCE_ID);
    let peer = fleet.a_daemon(PEER_INSTANCE_ID);
    verifier.discovers(&peer.instance_id).await;
    let genuine = peer.key.signer().mint_access(&the_operator());
    let first = verifier.verifies(&genuine).map(|claims| claims.login);

    // When its connection to the room ends and discovery clears the roster, as it does before
    // every reconnect — no await in between, so no discovery tick can refill it
    verifier.registry.clear();
    let roster = verifier.registry.snapshot_remotes();
    let while_reconnecting = verifier.verifies(&genuine).map(|claims| claims.login);

    // Then the token still verifies: a key id names exactly one key, so a remembered key can never
    // turn into a wrong one, and a reconnect must not log every peer's users out
    assert_eq!(
        (first, roster.len(), while_reconnecting),
        (Ok(the_operator().login), 0, Ok(the_operator().login))
    );
}

/// One LiveKit server, one common room named afresh for this run.
struct Fleet {
    livekit: LiveKitTestkit,
    ws_url: String,
    room: String,
    homes: tempfile::TempDir,
}

impl Fleet {
    async fn start() -> Self {
        let livekit = LiveKitTestkit::start()
            .await
            .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
        Self {
            ws_url: livekit.get_ws_url(),
            livekit,
            room: format!("key-trust-lobby-{}", uuid::Uuid::new_v4()),
            homes: tempfile::tempdir().expect("a temporary directory"),
        }
    }

    /// A daemon in the common room: its own keypair, the discovery loop advertising it, and the
    /// verifier `runtime::build` gives it — a key directory over the peers it discovers.
    fn a_daemon(&self, instance_id: &str) -> Daemon {
        let config = Arc::new(self.config_for(instance_id));
        let key = DaemonSigningKey::load_or_generate(&self.home_of(instance_id))
            .expect("a daemon generates its keypair");
        let registry = Arc::new(CommonRoomPeerRegistry::new());
        let room_slot = Arc::new(tokio::sync::RwLock::new(None));
        let directory = Arc::new(CommonRoomKeyDirectory::new(Arc::clone(&registry)));
        let tokens = SessionTokens::new(&key, directory);
        spawn_common_room_discovery_loop(
            Arc::clone(&config),
            Arc::clone(&registry),
            Arc::clone(&room_slot),
            advertised_signing_key(&key),
        );
        Daemon {
            instance_id: local_instance_id_for_config(&config),
            key,
            tokens,
            registry,
            room_slot,
        }
    }

    /// `token.TokenService` as a daemon registers it, for a caller whose session token verifies.
    fn the_client_facing_mint(&self) -> TokenServiceImpl<LiveKitTokenProvider> {
        let generator = TokenGenerator::new(
            LK_API_KEY.to_string(),
            LK_API_SECRET.to_string(),
            String::new(),
            String::new(),
            Duration::from_secs(600),
        );
        TokenServiceImpl::authenticated(
            LiveKitTokenProvider(Arc::new(generator)),
            Arc::new(|_session_token: &str| true),
        )
    }

    fn config_for(&self, instance_id: &str) -> DaemonConfig {
        let path = self.homes.path().join(format!("{instance_id}.yaml"));
        let yaml = format!(
            "daemon_instance_id: {instance_id}\n\
             livekit:\n  enabled: true\n  url: {}\n  api_key: {LK_API_KEY}\n  \
             api_secret: {LK_API_SECRET}\n  common_room: {}\n",
            self.ws_url, self.room
        );
        std::fs::write(&path, yaml).expect("write daemon.yaml");
        DaemonConfig::load(&path).expect("daemon.yaml loads")
    }

    fn home_of(&self, instance_id: &str) -> PathBuf {
        self.homes.path().join(instance_id).join(SIGNING_KEY_FILE)
    }
}

struct Daemon {
    instance_id: String,
    key: DaemonSigningKey,
    tokens: SessionTokens,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
}

impl Daemon {
    async fn discovers(&self, peer_instance_id: &str) {
        let registry = Arc::clone(&self.registry);
        eventually(
            &format!("{} discovers {peer_instance_id}", self.instance_id),
            DISCOVERY_TIMEOUT,
            || {
                let remotes = registry.snapshot_remotes();
                remotes
                    .iter()
                    .any(|row| row.instance_id.0 == peer_instance_id)
                    .then_some(())
                    .ok_or_else(|| format!("remotes: {remotes:?}"))
            },
        )
        .await;
    }

    /// Wait until this daemon's own connection to the common room carries `identity`'s
    /// `metadata`, then sync its registry from that room — so the verdict below is on a roster
    /// that demonstrably held the advertisement.
    async fn sees_metadata_of(&self, identity: &str, metadata: &str) {
        let deadline = tokio::time::Instant::now() + DISCOVERY_TIMEOUT;
        loop {
            if let Some(room) = self.room_slot.read().await.clone() {
                let seen = room
                    .remote_participants()
                    .values()
                    .any(|p| p.identity().to_string() == identity && p.metadata() == metadata);
                if seen {
                    self.registry.sync_from_room(&room, &self.instance_id);
                    return;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{} never saw {identity}'s advertisement on the common room",
                self.instance_id
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn verifies(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        self.tokens.verifier().verify_now(token)
    }
}

/// A keypair nobody but its holder knows — the impostor's.
struct Keypair {
    key: DaemonSigningKey,
    _home: tempfile::TempDir,
}

impl Keypair {
    fn generate() -> Self {
        let home = tempfile::tempdir().expect("a temporary directory");
        let key = DaemonSigningKey::load_or_generate(&home.path().join(SIGNING_KEY_FILE))
            .expect("a keypair is generated");
        Self { key, _home: home }
    }

    /// The common-room metadata a daemon named `instance_id` holding this key would publish.
    fn advertisement_as(&self, instance_id: &str) -> String {
        let advertised = advertised_signing_key(&self.key);
        advertisement_json(instance_id, advertised.key_id, advertised.public_key)
    }

    /// Metadata advertising this key's bytes under `victim`'s key id.
    fn advertisement_claiming(&self, instance_id: &str, victim: &DaemonSigningKey) -> String {
        advertisement_json(
            instance_id,
            victim.key_id().to_string(),
            advertised_signing_key(&self.key).public_key,
        )
    }
}

/// The common-room metadata a daemon named `instance_id` would publish, advertising the given key.
fn advertisement_json(
    instance_id: &str,
    signing_key_id: String,
    signing_public_key: String,
) -> String {
    daemon_metadata_json(
        &DaemonAdvertisement {
            instance_id: instance_id.to_string(),
            label: "Build server".to_string(),
            repos_base_path: String::new(),
            max_attachment_bytes: 0,
            sandboxed_codebase: None,
            signing_key_id,
            signing_public_key,
        },
        instance_id,
    )
    .expect("an advertisement serializes")
}

async fn mint_room_token(
    mint: &TokenServiceImpl<LiveKitTokenProvider>,
    room: &str,
    identity: &str,
) -> Result<String, tddy_rpc::Status> {
    mint.generate_token(Request::new(GenerateTokenRequest {
        room: room.to_string(),
        identity: identity.to_string(),
        session_token: "an-access-token".to_string(),
    }))
    .await
    .map(|response| response.into_inner().token)
}

/// Join the room with `jwt` and publish `metadata`. The connection lasts as long as the returned
/// room and its event stream are held.
async fn join_advertising(
    ws_url: &str,
    jwt: &str,
    metadata: &str,
) -> (
    Room,
    tokio::sync::mpsc::UnboundedReceiver<livekit::RoomEvent>,
) {
    let (room, events) = Room::connect(ws_url, jwt, RoomOptions::default())
        .await
        .expect("the minted JWT admits its holder to the common room");
    // `set_metadata` can report a timeout in livekit 0.7 even when the update reached the room
    // (see `livekit_peer_discovery`'s module docs), so its result is not the evidence: the
    // verifying daemon's roster is, and `Daemon::sees_metadata_of` waits for it.
    let _ = room
        .local_participant()
        .set_metadata(metadata.to_string())
        .await;
    (room, events)
}

fn the_operator() -> GitHubUser {
    GitHubUser {
        id: 4242,
        login: "testuser".to_string(),
        avatar_url: String::new(),
        name: "Test User".to_string(),
    }
}
