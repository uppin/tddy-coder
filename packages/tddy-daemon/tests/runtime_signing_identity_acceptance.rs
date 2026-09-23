//! Acceptance: the daemon `runtime::build` assembles advertises, on its common room, the key it signs
//! session tokens with.
//!
//! Product contract: `docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-signing-key.md`.
//!
//! The cross-host suites wire a key directory and an advertised key by hand, the way `runtime.rs`
//! does — so a regression in the runtime's own wiring (a daemon that advertises no key, or one other
//! than the key it signs with) would leave them green while every peer refused its tokens. This
//! suite builds the daemon exactly as the binary and Tddy Desktop do, starts its tasks, and reads
//! its advertisement off the room the way a peer would.
//!
//! Needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`); `#[serial]` so it owns
//! the container alone.

use std::time::Duration;

use livekit::prelude::RoomOptions;
use livekit::Room;
use serial_test::serial;
use tddy_daemon::common_room_key_directory::advertised_signing_key;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_daemon_livekit::livekit_peer_discovery::{
    local_instance_id_for_config, parse_peer_daemon_json,
};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_testing_commons::wait::eventually_awaiting;

const INSTANCE_ID: &str = "runtime-signing-identity";
const ADVERTISEMENT_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::test]
#[serial]
async fn a_built_daemon_advertises_the_key_it_signs_session_tokens_with() {
    // Given a daemon with users to authenticate and a common room to join, built and started as a
    // host starts it
    let livekit = LiveKitTestkit::start()
        .await
        .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
    let room = format!("runtime-signing-lobby-{}", uuid::Uuid::new_v4());
    let data_dir = tempfile::tempdir().expect("a data directory");
    let config = a_daemon_config(&livekit.get_ws_url(), &room, data_dir.path());
    let daemon = runtime::build(config.clone(), RuntimeOptions::for_embedded())
        .await
        .expect("the runtime builds");
    let tasks = daemon.tasks.spawn();

    // When a peer in that room reads the daemon's advertisement
    let (observer, _events) = Room::connect(
        &livekit.get_ws_url(),
        &livekit
            .generate_token(&room, "web-runtime-observer")
            .expect("an observer token"),
        RoomOptions::default(),
    )
    .await
    .expect("the observer joins the common room");
    let discovery_identity = local_instance_id_for_config(&config);
    let advertised = eventually_awaiting(
        "the daemon publishes its advertisement",
        ADVERTISEMENT_TIMEOUT,
        || {
            let metadata = observer
                .remote_participants()
                .values()
                .find(|p| p.identity().to_string() == discovery_identity)
                .map(|p| p.metadata())
                .unwrap_or_default();
            std::future::ready(
                parse_peer_daemon_json(&metadata)
                    .map(|peer| peer.advertisement)
                    .map_err(|e| format!("metadata {metadata:?}: {e}")),
            )
        },
    )
    .await;
    tasks.abort_all();

    // Then it names, and carries, the key the daemon signs with — the one its data directory holds
    let signing_key = tddy_daemon_auth::load_signing_key(&config)
        .expect("the key the runtime generated is loadable");
    let expected = advertised_signing_key(&signing_key);
    assert_eq!(
        (advertised.signing_key_id, advertised.signing_public_key),
        (expected.key_id, expected.public_key)
    );
}

fn a_daemon_config(ws_url: &str, room: &str, data_dir: &std::path::Path) -> DaemonConfig {
    let os_user = std::env::var("USER").expect("USER required");
    let yaml = format!(
        r#"
daemon_instance_id: {INSTANCE_ID}
listen:
  web_port: 0
  web_host: 127.0.0.1
github:
  stub: true
users:
  - github_user: "testuser"
    os_user: "{os_user}"
livekit:
  enabled: true
  url: {ws_url}
  api_key: devkey
  api_secret: secret
  common_room: {room}
"#
    );
    let mut config: DaemonConfig = serde_yaml::from_str(&yaml).expect("the config fixture parses");
    config.tddy_data_dir = Some(data_dir.to_path_buf());
    config
}
