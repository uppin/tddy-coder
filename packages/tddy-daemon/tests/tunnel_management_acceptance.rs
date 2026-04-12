//! Acceptance: managed tunnel Connect RPC on the daemon mux (same stack as ConnectionService).
//!
//! Exercises list/start/stop against a real [`TunnelSupervisor`] wired through the Connect router.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use prost::Message;
use tower::ServiceExt;

use tddy_connectrpc::connect_router;
use tddy_daemon::tunnel_management_rpc::tunnel_management_rpc_entry_with_supervisor;
use tddy_daemon::tunnel_supervisor::TunnelSupervisor;
use tddy_rpc::{MultiRpcService, RpcBridge};
use tddy_service::proto::tunnel_management::{
    ListTunnelAdvertisementsRequest, ListTunnelAdvertisementsResponse, StartTunnelRequest,
    StopTunnelRequest, StopTunnelResponse, TunnelBindingState, TunnelKind,
};

fn tunnel_rpc_app() -> axum::Router {
    tunnel_rpc_app_with_supervisor(Arc::new(TunnelSupervisor::new()))
}

fn tunnel_rpc_app_with_supervisor(supervisor: Arc<TunnelSupervisor>) -> axum::Router {
    connect_router(RpcBridge::new(MultiRpcService::new(vec![
        tunnel_management_rpc_entry_with_supervisor(supervisor),
    ])))
}

#[tokio::test]
async fn daemon_tunnel_rpc_lists_advertisements_when_binding_pending() {
    let supervisor = Arc::new(TunnelSupervisor::new());
    supervisor.ingest_pending_codex_oauth("acceptance-session-pending-1", 9876, None);
    let app = tunnel_rpc_app_with_supervisor(supervisor);
    let req = ListTunnelAdvertisementsRequest {};
    let request = Request::builder()
        .method("POST")
        .uri("/rpc/tunnel_management.TunnelManagementService/ListTunnelAdvertisements")
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(req.encode_to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "ListTunnelAdvertisements must return 200 when RPC is registered"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp = ListTunnelAdvertisementsResponse::decode(&body[..]).expect("decode list response");

    const EXPECT_PORT: u32 = 9_876;
    const SESSION: &str = "acceptance-session-pending-1";

    assert_eq!(
        resp.advertisements.len(),
        1,
        "expected exactly one PENDING advertisement after supervisor ingests tunnel metadata"
    );
    let a = &resp.advertisements[0];
    assert_eq!(a.operator_loopback_port, EXPECT_PORT);
    assert_eq!(a.session_correlation_id, SESSION);
    assert_eq!(a.kind, TunnelKind::CodexOauth as i32);
    assert_eq!(a.state, TunnelBindingState::Pending as i32);
}

#[tokio::test]
async fn daemon_tunnel_rpc_start_rejects_port_below_1024() {
    let app = tunnel_rpc_app();
    let req = StartTunnelRequest {
        session_correlation_id: "acceptance-low-port".to_string(),
        operator_bind_port: 80,
        kind: TunnelKind::CodexOauth as i32,
    };
    let request = Request::builder()
        .method("POST")
        .uri("/rpc/tunnel_management.TunnelManagementService/StartTunnel")
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(req.encode_to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "privileged / low ports must map to HTTP 400 (invalid_argument or failed_precondition)"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("Connect error JSON");
    let code = json["code"].as_str().expect("error code");
    assert!(
        code == "invalid_argument" || code == "failed_precondition",
        "expected invalid_argument or failed_precondition for operator_bind_port < 1024, got {code:?}"
    );
}

#[tokio::test]
async fn daemon_tunnel_rpc_stop_returns_idle_state() {
    let app = tunnel_rpc_app();
    let req = StopTunnelRequest {
        session_correlation_id: "acceptance-stop-1".to_string(),
    };
    let request = Request::builder()
        .method("POST")
        .uri("/rpc/tunnel_management.TunnelManagementService/StopTunnel")
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(req.encode_to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "StopTunnel must succeed with protobuf body when tunnel supervisor is wired"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp = StopTunnelResponse::decode(&body[..]).expect("decode stop response");
    assert_eq!(
        resp.state,
        TunnelBindingState::Idle as i32,
        "StopTunnel must report IDLE binding state"
    );
}
