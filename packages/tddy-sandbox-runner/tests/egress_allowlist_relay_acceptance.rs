//! Acceptance: the host relay enforces the jail's egress policy on every `TunnelOpen` on its own —
//! a jailed process that skips the in-jail shim and speaks the relay protocol directly is still
//! refused, and the refusal names the session, the host and the tool set that lacked it.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).
//!
//! The jail side is the fake `SandboxService` in `common`, which pushes a raw `TunnelOpen` exactly
//! as a process bypassing the shim would. The relay under test is the real one.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Captured, Mode};
use tddy_sandbox::{EgressPolicy, HostPattern};
use tddy_sandbox_runner::{run_host_relay, EgressGuard, HostRelayConfig, NullToolHandler};
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const SESSION_ID: &str = "egress-allowlist-session";

/// A loopback listener that counts the connections it accepted.
struct AListener {
    port: u16,
    accepted: Arc<AtomicUsize>,
}

async fn a_local_listener() -> AListener {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&accepted);
    tokio::spawn(async move {
        while listener.accept().await.is_ok() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    AListener { port, accepted }
}

fn a_policy_admitting_only(port: u16) -> EgressPolicy {
    EgressPolicy::Hosts(vec![HostPattern::new("127.0.0.1", port)])
}

/// Start the real relay against a fake jail that sends one `TunnelOpen` for `127.0.0.1:port`
/// straight down the `SessionChannel`, under `egress`; return what the jail was answered.
async fn a_tunnel_open_straight_to_the_relay(
    port: u16,
    egress: EgressGuard,
) -> Arc<Mutex<Captured>> {
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushTunnelOpen {
        host: "127.0.0.1".to_string(),
        port,
    })
    .await;
    let client = SandboxServiceClient::connect(endpoint)
        .await
        .expect("connect fake sandbox grpc");
    let (terminal_tx, _terminal_rx) = mpsc::unbounded_channel::<Bytes>();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
    // Dropping the handle detaches the relay; it lives as long as the channel does.
    let _relay = run_host_relay(
        client,
        NullToolHandler,
        HostRelayConfig::new(SESSION_ID, terminal_tx).with_egress(egress),
        stdin_rx,
    )
    .await
    .expect("start host relay");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while captured.lock().unwrap().tunnel_acks.is_empty() && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    captured
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tunnel_open_sent_straight_to_the_host_relay_for_an_undeclared_host_is_refused() {
    // Given — the jail's tools declare one local host; a second one is listening too
    let declared = a_local_listener().await;
    let undeclared = a_local_listener().await;
    let egress = EgressGuard::new(
        a_policy_admitting_only(declared.port),
        SESSION_ID,
        vec!["curl".into()],
    );

    // When — the jail asks the relay for the undeclared one, bypassing the shim
    let captured = a_tunnel_open_straight_to_the_relay(undeclared.port, egress).await;

    // Then — refused, and the host never dialed it
    let acks = captured.lock().unwrap().tunnel_acks.clone();
    assert_eq!(acks.iter().map(|a| a.ok).collect::<Vec<_>>(), vec![false]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(undeclared.accepted.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tunnel_open_for_a_declared_host_is_dialed_under_a_host_policy() {
    // Given — the positive control for the refusal above
    let declared = a_local_listener().await;
    let egress = EgressGuard::new(
        a_policy_admitting_only(declared.port),
        SESSION_ID,
        vec!["curl".into()],
    );

    // When
    let captured = a_tunnel_open_straight_to_the_relay(declared.port, egress).await;

    // Then
    let acks = captured.lock().unwrap().tunnel_acks.clone();
    assert_eq!(
        acks.iter()
            .map(|a| (a.ok, a.error.clone()))
            .collect::<Vec<_>>(),
        vec![(true, String::new())]
    );
    assert_eq!(declared.accepted.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_host_is_logged_with_the_tool_set_that_lacked_it() {
    // Given — a jail of two tools, only one of which declares a host
    let declared = a_local_listener().await;
    let undeclared = a_local_listener().await;
    let egress = EgressGuard::new(
        a_policy_admitting_only(declared.port),
        SESSION_ID,
        vec!["curl".into(), "jq".into()],
    );

    // When
    let captured = a_tunnel_open_straight_to_the_relay(undeclared.port, egress).await;

    // Then — the refusal the jail is answered with is the line the relay logs
    let acks = captured.lock().unwrap().tunnel_acks.clone();
    assert_eq!(
        acks.iter().map(|a| a.error.clone()).collect::<Vec<_>>(),
        vec![format!(
            "egress to 127.0.0.1:{} refused for session {SESSION_ID}: not declared by tool set [curl, jq]",
            undeclared.port
        )]
    );
}
