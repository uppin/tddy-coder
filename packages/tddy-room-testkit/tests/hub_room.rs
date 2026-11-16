//! The in-memory room hub: discovery, classification, RPC and broadcast over one room.

use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::{
    daemon_metadata_json, AdvertisedSigningKey, CommonRoomPeerRegistry, DaemonAdvertisement,
};
use tddy_room_testkit::RoomHub;
use tddy_rpc::{RequestTransport, RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tokio::sync::mpsc;

fn config_with_instance_id(instance_id: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, format!("daemon_instance_id: {instance_id}\n")).unwrap();
    std::fs::write(dir.path().join("users.yaml"), "[]\n").unwrap();
    let config = DaemonConfig::load(&path).unwrap();
    (dir, config)
}

fn advertisement_metadata(instance_id: &str) -> String {
    let advertisement = DaemonAdvertisement {
        instance_id: instance_id.to_string(),
        label: format!("{instance_id} (this daemon)"),
        repos_base_path: String::new(),
        max_attachment_bytes: 0,
        sandboxed_codebase: None,
        signing_key_id: String::new(),
        signing_public_key: String::new(),
    };
    daemon_metadata_json(&advertisement, instance_id).unwrap()
}

struct EchoService;

#[async_trait::async_trait]
impl RpcService for EchoService {
    async fn handle_rpc(&self, _service: &str, method: &str, request: &RpcMessage) -> RpcResult {
        match method {
            "Echo" => {
                // The stamp is part of the contract: a caller over the hub sees LiveKit, with
                // the caller's identity, as a caller over a real room does.
                assert_eq!(request.metadata.transport(), RequestTransport::LiveKit);
                let mut body = request.payload.clone();
                body.extend_from_slice(
                    request
                        .metadata
                        .sender_identity
                        .as_deref()
                        .unwrap_or("")
                        .as_bytes(),
                );
                RpcResult::Unary(Ok(body))
            }
            "Shout" => {
                let (tx, rx) = mpsc::channel(4);
                let body = request.payload.clone();
                tokio::spawn(async move {
                    for _ in 0..3 {
                        let _ = tx.send(Ok(body.clone())).await;
                    }
                });
                RpcResult::ServerStream(Ok(rx))
            }
            _ => RpcResult::Unary(Err(Status::unimplemented("no such method"))),
        }
    }
}

#[tokio::test]
async fn two_daemons_discover_each_other_and_forward_a_call() {
    let hub = RoomHub::new();
    let room = "hub-test-common-room";
    let (_dir_b, config_b) = config_with_instance_id("hub-peer-b");
    let (_dir_a, config_a) = config_with_instance_id("hub-peer-a");

    // B advertises; A attaches its registry to the room.
    let _b = hub.advertise_daemon(room, &config_b, &AdvertisedSigningKey::default());
    let registry_a = Arc::new(CommonRoomPeerRegistry::new());
    let _attach = hub.attach_registry(room, "hub-peer-a", registry_a.clone());

    // Wait until A's registry sees B, classified as a daemon.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if registry_a
                .snapshot_remotes()
                .iter()
                .any(|d| d.instance_id.0 == "hub-peer-b")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("A's registry should see B");

    // B serves under daemon-{instance_id}; the forwarder addresses it from the bare peer id.
    let rpc_b = hub.join(room, "daemon-hub-peer-b", "");
    rpc_b.serve(Arc::new(EchoService));

    let forwarder =
        tddy_room_testkit::HubPeerForwarder::new(hub.clone(), room, "daemon-hub-peer-a");
    let response = forwarder_call(&forwarder).await;
    assert_eq!(response, b"pingdaemon-hub-peer-a".to_vec());

    // The eligible source lists A (local) and B, as the LiveKit source does.
    let source = tddy_room_testkit::HubEligibleDaemonSource::new(
        Arc::new(config_a),
        registry_a,
        tddy_daemon_kernel::peer_forwarding::CommonRoom::new(forwarder, &config_b),
    );
    let eligible =
        tddy_host_service::multi_host::EligibleDaemonSource::list_eligible_daemons(&*source);
    let ids: Vec<&str> = eligible.iter().map(|d| d.instance_id.0.as_str()).collect();
    assert!(ids.contains(&"hub-peer-b"), "eligible: {ids:?}");
}

async fn forwarder_call(forwarder: &Arc<tddy_room_testkit::HubPeerForwarder>) -> Vec<u8> {
    use tddy_daemon_extensions::PeerForwarder;
    forwarder
        .call_unary("hub-peer-b", "test.Echo", "Echo", b"ping".to_vec())
        .await
        .expect("forwarded call")
}

#[tokio::test]
async fn a_browser_identity_is_never_a_daemon_and_a_departure_clears_the_registry() {
    let hub = RoomHub::new();
    let room = "hub-test-classification";
    let registry = Arc::new(CommonRoomPeerRegistry::new());
    let _attach = hub.attach_registry(room, "hub-local", registry.clone());

    let browser = hub.join(
        room,
        "web-client-1",
        &advertisement_metadata("web-client-1"),
    );
    let daemon = hub.join(
        room,
        "hub-daemon-1",
        &advertisement_metadata("hub-daemon-1"),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !registry.snapshot_remotes().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("the daemon should be discovered");
    let remotes = registry.snapshot_remotes();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].instance_id.0, "hub-daemon-1");

    daemon.leave();
    drop(browser);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if registry.snapshot_remotes().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("a departure should clear the registry");
}

#[tokio::test]
async fn broadcast_reaches_subscribers_with_the_source_identity() {
    let hub = RoomHub::new();
    let room = "hub-test-broadcast";
    let mut rx = hub.subscribe(room, "announcements");
    let speaker = hub.join(room, "hub-daemon-1", "");
    speaker.publish("announcements", b"hello".to_vec());
    let (source, payload) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("a broadcast should arrive")
        .expect("the channel stays open");
    assert_eq!(source, "hub-daemon-1");
    assert_eq!(payload, b"hello".to_vec());
}

#[tokio::test]
async fn a_call_to_an_absent_participant_is_unavailable_not_a_hang() {
    let hub = RoomHub::new();
    let client = hub.client("hub-test-absent", "daemon-a", "daemon-nobody");
    let err = client
        .call_unary("test.Echo", "Echo", b"ping".to_vec())
        .await
        .expect_err("an absent participant cannot answer");
    assert_eq!(err.code(), tddy_rpc::Code::Unavailable);
}
