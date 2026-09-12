//! Where the daemon's LiveKit surface answers after `#unbundle` node 4 moved family T.
//!
//! Two halves of one change, and both have to hold or the split is only half done: the coordinate
//! `livekit.LiveKitService` has to be *served* — a proto nothing registers is a file, not a service
//! — and `the pre-unbundle monolithic RPC coordinate` has to have stopped answering `StreamLiveKitRooms`,
//! because while both answer a client can keep calling the old one and the move never lands.
//!
//! Read from the runtime the binary actually builds, and by *dispatching* rather than by reading a
//! name list: what a client can reach is what the bootstrap registered and what that service will
//! route, and a test that assembled its own roster would go on passing after the bootstrap stopped
//! registering it.

use prost::Message as _;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_rpc::{Code, RpcMessage, RpcResult, ServiceEntry};
use tddy_service::proto::livekit::StreamLiveKitRoomsRequest;

/// A fully configured daemon: a `github` block, because the bootstrap only reaches the block that
/// registers these services once `build_auth_entries` has given it a session resolver; a user
/// mapping for that resolver to resolve; and a LiveKit block, so nothing below passes because a
/// conditional registration was skipped. The stub provider, so no test ever reaches GitHub.
fn a_daemon_config_with_a_livekit_block() -> DaemonConfig {
    serde_yaml::from_str(
        r#"
listen:
  web_port: 0
  web_host: 127.0.0.1
github:
  stub: true
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_tools:
  - path: /bin/true
    label: t
livekit:
  enabled: true
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  common_room: tddy-lobby
"#,
    )
    .expect("the config fixture did not parse")
}

async fn a_built_daemon() -> Vec<ServiceEntry> {
    runtime::build(
        a_daemon_config_with_a_livekit_block(),
        RuntimeOptions::for_embedded(),
    )
    .await
    .expect("the runtime did not build")
    .entries
}

fn entry_named<'a>(entries: &'a [ServiceEntry], name: &str) -> &'a ServiceEntry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| {
            let served: Vec<&str> = entries.iter().map(|e| e.name).collect();
            panic!("{name} is not registered; the daemon serves: {served:?}")
        })
}

/// A `StreamLiveKitRooms` call carrying a token no daemon ever issued.
///
/// Deliberately invalid: what is under test is *routing*, and an unauthenticated refusal proves the
/// method was found and reached its handler just as well as a snapshot would — without needing a
/// LiveKit server to answer.
fn a_rooms_subscription() -> RpcMessage {
    RpcMessage::new(
        StreamLiveKitRoomsRequest {
            session_token: "not-a-real-token".to_string(),
        }
        .encode_to_vec(),
        Default::default(),
    )
}

fn status_of(result: RpcResult) -> tddy_rpc::Status {
    match result {
        RpcResult::Unary(Err(status)) => status,
        RpcResult::ServerStream(Err(status)) => status,
        _ => panic!("expected the call to be refused, since the token is not a real one"),
    }
}

#[tokio::test]
async fn serves_the_rooms_stream_as_its_own_service() {
    // Given the runtime the daemon boots with
    let entries = a_built_daemon().await;

    // When the new coordinate is called
    let refusal = status_of(
        entry_named(&entries, "livekit.LiveKitService")
            .service
            .handle_rpc(
                "livekit.LiveKitService",
                "StreamLiveKitRooms",
                &a_rooms_subscription(),
            )
            .await,
    );

    // Then it reached a handler and that handler judged the token — the method is routed, not
    // merely declared
    assert_eq!(
        refusal.code(),
        Code::Unauthenticated,
        "livekit.LiveKitService did not route StreamLiveKitRooms to a handler: {refusal:?}"
    );
}

#[tokio::test]
async fn no_longer_answers_the_rooms_stream_on_the_connection_service() {
    // Given the same runtime
    let entries = a_built_daemon().await;

    // When the old coordinate is called
    let refusal = status_of(
        entry_named(&entries, "the pre-unbundle monolithic RPC coordinate")
            .service
            .handle_rpc(
                "the pre-unbundle monolithic RPC coordinate",
                "StreamLiveKitRooms",
                &a_rooms_subscription(),
            )
            .await,
    );

    // Then the daemon does not know the method there any more. `NOT_FOUND` rather than
    // `UNAUTHENTICATED` is the whole assertion: an unauthenticated refusal would mean the handler
    // is still mounted and only the credential was wrong.
    assert_eq!(
        refusal.code(),
        Code::NotFound,
        "the pre-unbundle monolithic RPC coordinate still answers StreamLiveKitRooms: {refusal:?}"
    );
}
