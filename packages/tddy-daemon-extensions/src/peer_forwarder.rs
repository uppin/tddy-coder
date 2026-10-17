//! Forwarding an RPC to a peer daemon, as a port.
//!
//! The kernel's `CommonRoom` owns the forwarding *policy* — the deadline a unary call gets, the
//! stream's idle timeout, the errors a caller sees. What it needs from a transport is only this: is
//! a peer reachable at all, and carry one call to `daemon-{peer_id}`. The LiveKit integration
//! implements it over the common room; a daemon with no integration has [`NoPeerForwarding`].

use tddy_rpc::Status;
use tokio::sync::mpsc;

/// The transport that carries a forwarded call to a peer daemon.
#[async_trait::async_trait]
pub trait PeerForwarder: Send + Sync {
    /// Whether a peer can be reached at all right now — the transport is up.
    async fn is_connected(&self) -> bool;

    /// One unary call to `daemon-{peer_id}`. No deadline: the caller applies it.
    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status>;

    /// A server stream from `daemon-{peer_id}`. The receiver stays fed until the peer ends the
    /// stream or fails it; the implementation keeps whatever the stream needs alive until then.
    ///
    /// The caller MUST drain the receiver promptly, or drop it: a reader that stalls backs the
    /// transport up, and a shared transport stalls every other in-flight call with it.
    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>;
}

/// No transport wired: no peer is connected, and every forward is a failed precondition.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPeerForwarding;

#[async_trait::async_trait]
impl PeerForwarder for NoPeerForwarding {
    async fn is_connected(&self) -> bool {
        false
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        Err(not_wired(peer_id, service, method))
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(not_wired(peer_id, service, method))
    }
}

fn not_wired(peer_id: &str, service: &str, method: &str) -> Status {
    Status::failed_precondition(format!(
        "no peer transport is wired on this daemon; cannot forward {service}/{method} to daemon {peer_id}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_nothing_wired_no_peer_is_connected() {
        assert!(!NoPeerForwarding.is_connected().await);
    }

    #[tokio::test]
    async fn with_nothing_wired_a_forward_is_a_failed_precondition_naming_the_call() {
        // When a call is forwarded with no transport
        let refused = NoPeerForwarding
            .call_unary(
                "peer-1",
                "project.ProjectService",
                "AddProjectToHost",
                Vec::new(),
            )
            .await
            .expect_err("nothing can carry the call");

        // Then it is refused as a precondition, naming what could not be forwarded
        assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
        assert_eq!(
            refused.message,
            "no peer transport is wired on this daemon; cannot forward project.ProjectService/AddProjectToHost to daemon peer-1"
        );
    }
}
