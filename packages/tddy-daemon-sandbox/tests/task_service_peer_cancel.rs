//! A `CancelTask` addressed to a task on another daemon is forwarded to that daemon — how cancelling
//! a split start reaches the codebase host's half of it.
//!
//! The peer is an in-memory `PeerForwarder` behind a real `CommonRoom`; no LiveKit.
//!
//! Feature docs: docs/ft/daemon/background-tasks.md (§ RPC surface)

use std::sync::{Arc, Mutex};

use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_sandbox::task_service::TaskServiceImpl;
use tddy_rpc::{Request, Status};
use tddy_service::proto::tasks::{CancelTaskRequest, CancelTaskResponse, TaskService};
use tokio::sync::mpsc;

const THIS_DAEMON: &str = "workstation-a";
const PEER: &str = "workstation-b";

/// A peer that answers `CancelTask` with `ok` and records what it was asked.
#[derive(Default)]
struct APeerRunningTheTask {
    cancelled: Mutex<Vec<(String, CancelTaskRequest)>>,
}

#[async_trait::async_trait]
impl PeerForwarder for APeerRunningTheTask {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        assert_eq!((service, method), ("tasks.TaskService", "CancelTask"));
        self.cancelled.lock().unwrap().push((
            peer_id.to_string(),
            CancelTaskRequest::decode(body.as_slice()).unwrap(),
        ));
        Ok(CancelTaskResponse {
            ok: true,
            message: String::new(),
        }
        .encode_to_vec())
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        _service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        panic!("cancelling a task opens no stream, but {method} was opened")
    }
}

fn a_task_service_forwarding_to(peer: Arc<APeerRunningTheTask>) -> TaskServiceImpl {
    let room = CommonRoom::new(peer as Arc<dyn PeerForwarder>, &DaemonConfig::default());
    TaskServiceImpl::new(
        tddy_task::TaskRegistry::new(),
        Arc::new(|token: &str| (token == "valid").then(|| "operator".to_string())),
    )
    .forwarding_to_peers(room, THIS_DAEMON)
}

#[tokio::test]
async fn cancel_task_for_a_peer_daemon_is_forwarded_to_it() {
    // Given
    let peer = Arc::new(APeerRunningTheTask::default());
    let service = a_task_service_forwarding_to(Arc::clone(&peer));

    // When — the operator cancels a task the peer runs
    let answer = service
        .cancel_task(Request::direct(CancelTaskRequest {
            session_token: "valid".to_string(),
            task_id: "start-task-on-b".to_string(),
            daemon_instance_id: PEER.to_string(),
        }))
        .await
        .expect("the peer's answer comes back")
        .into_inner();

    // Then — the peer was asked, for that task, and its answer is the caller's
    assert!(answer.ok, "the peer cancelled it");
    let asked = peer.cancelled.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "exactly one forward");
    assert_eq!(
        (asked[0].0.as_str(), asked[0].1.task_id.as_str()),
        (PEER, "start-task-on-b")
    );
}

#[tokio::test]
async fn cancel_task_for_this_daemon_is_not_forwarded() {
    // Given
    let peer = Arc::new(APeerRunningTheTask::default());
    let service = a_task_service_forwarding_to(Arc::clone(&peer));

    // When — a task id this daemon does not run, addressed to this daemon
    let refused = service
        .cancel_task(Request::direct(CancelTaskRequest {
            session_token: "valid".to_string(),
            task_id: "no-such-task".to_string(),
            daemon_instance_id: THIS_DAEMON.to_string(),
        }))
        .await
        .expect_err("no such task here");

    // Then
    assert_eq!(refused.code, tddy_rpc::Code::NotFound);
    assert!(
        peer.cancelled.lock().unwrap().is_empty(),
        "nothing was forwarded"
    );
}
