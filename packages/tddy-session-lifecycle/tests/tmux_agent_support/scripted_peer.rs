//! A peer daemon in the common room, scripted: it answers what a close and a delete forward to it —
//! `PreviewCloseSession`, `CloseSession`, `DeleteSession`, `CancelAgentConversation` — as a test
//! told it to, and records every call it was asked (docs/ft/daemon/session-liveness.md § Closing a
//! session). No LiveKit: the room forwards through this [`PeerForwarder`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use prost::Message;
use tddy_daemon_extensions::PeerForwarder;
use tddy_rpc::Status;
use tokio::sync::mpsc;

/// The one peer of the room, by the id a session's record names it with.
pub const PEER: &str = "peer-daemon-1";

/// Answers each method as scripted; a method nobody scripted is `UNIMPLEMENTED`.
#[derive(Default)]
pub struct ScriptedPeer {
    gone: AtomicBool,
    unary: Mutex<HashMap<String, Result<Vec<u8>, Status>>>,
    streams: Mutex<HashMap<String, Vec<Vec<u8>>>>,
    asked: Mutex<Vec<(String, Vec<u8>)>>,
}

impl ScriptedPeer {
    pub fn in_the_room() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every `method` call is answered `answer`.
    pub fn answers<M: Message>(&self, method: &str, answer: Result<M, Status>) {
        self.unary.lock().unwrap().insert(
            method.to_string(),
            answer.map(|message| message.encode_to_vec()),
        );
    }

    /// Every `method` stream carries `frames`, then ends.
    pub fn streams<M: Message>(&self, method: &str, frames: Vec<M>) {
        self.streams.lock().unwrap().insert(
            method.to_string(),
            frames.iter().map(Message::encode_to_vec).collect(),
        );
    }

    /// From now on the room is down: no peer can be reached.
    pub fn leaves_the_room(&self) {
        self.gone.store(true, Ordering::SeqCst);
    }

    /// Every `method` call the peer was asked, decoded as `M`, in order.
    pub fn was_asked<M: Message + Default>(&self, method: &str) -> Vec<M> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|(asked, _)| asked == method)
            .map(|(_, body)| M::decode(body.as_slice()).expect("a request the peer can decode"))
            .collect()
    }

    fn record(&self, peer_id: &str, method: &str, body: Vec<u8>) -> Result<(), Status> {
        if peer_id != PEER {
            return Err(Status::failed_precondition(format!(
                "no daemon {peer_id} in the common room"
            )));
        }
        self.asked.lock().unwrap().push((method.to_string(), body));
        Ok(())
    }
}

#[async_trait::async_trait]
impl PeerForwarder for ScriptedPeer {
    async fn is_connected(&self) -> bool {
        !self.gone.load(Ordering::SeqCst)
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        _service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.record(peer_id, method, body)?;
        self.unary
            .lock()
            .unwrap()
            .get(method)
            .cloned()
            .unwrap_or_else(|| Err(Status::unimplemented(format!("{method} is not scripted"))))
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        _service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.record(peer_id, method, body)?;
        let frames = self
            .streams
            .lock()
            .unwrap()
            .get(method)
            .cloned()
            .ok_or_else(|| Status::unimplemented(format!("{method} is not scripted")))?;
        let (tx, rx) = mpsc::channel(frames.len().max(1));
        for frame in frames {
            tx.send(Ok(frame)).await.expect("the receiver is held");
        }
        Ok(rx)
    }
}
