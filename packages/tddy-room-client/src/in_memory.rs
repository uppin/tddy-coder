//! A [`RoomConnector`] for tests of code that joins rooms: it records each join and answers from a
//! script — a session, an absent peer, or an unreachable room.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::{RoomConnectError, RoomConnector, RoomDuplex, RoomJoin, RoomSession};

/// What a scripted join answers.
pub enum ScriptedJoin {
    /// The peer is there; calls are answered by `rpc`.
    Joined(Arc<dyn tddy_rpc::RpcClientTransport>),
    /// The room was joined and the peer never came.
    PeerAbsent,
    /// The room could not be joined.
    Unreachable(String),
}

/// Records every join and answers each with the script.
pub struct InMemoryRoomConnector {
    script: Mutex<Option<ScriptedJoin>>,
    joins: Mutex<Vec<(RoomJoin, String, Duration)>>,
}

impl InMemoryRoomConnector {
    pub fn answering(join: ScriptedJoin) -> Self {
        Self {
            script: Mutex::new(Some(join)),
            joins: Mutex::new(Vec::new()),
        }
    }

    /// Every join asked for: what, for which peer, with how much patience.
    pub fn joins(&self) -> Vec<(RoomJoin, String, Duration)> {
        self.joins.lock().unwrap().clone()
    }
}

struct InMemorySession {
    rpc: Arc<dyn tddy_rpc::RpcClientTransport>,
}

impl RoomSession for InMemorySession {
    fn rpc(&self) -> Arc<dyn tddy_rpc::RpcClientTransport> {
        Arc::clone(&self.rpc)
    }

    fn open_duplex(&self, service: &str, method: &str) -> Result<RoomDuplex, tddy_rpc::Status> {
        Err(tddy_rpc::Status::unimplemented(format!(
            "the in-memory room scripts no duplex for {service}/{method}"
        )))
    }

    fn subscribe(&self, _topic: &str) -> mpsc::UnboundedReceiver<Vec<u8>> {
        mpsc::unbounded_channel().1
    }

    fn peer_present(&self) -> bool {
        true
    }
}

#[async_trait::async_trait]
impl RoomConnector for InMemoryRoomConnector {
    async fn connect(
        &self,
        join: &RoomJoin,
        peer_identity: &str,
        wait: Duration,
    ) -> Result<Arc<dyn RoomSession>, RoomConnectError> {
        self.joins
            .lock()
            .unwrap()
            .push((join.clone(), peer_identity.to_string(), wait));
        match self
            .script
            .lock()
            .unwrap()
            .take()
            .expect("the in-memory connector was asked to join more times than it was scripted for")
        {
            ScriptedJoin::Joined(rpc) => Ok(Arc::new(InMemorySession { rpc })),
            ScriptedJoin::PeerAbsent => Err(RoomConnectError::PeerAbsent {
                identity: peer_identity.to_string(),
                waited: wait,
            }),
            ScriptedJoin::Unreachable(reason) => Err(RoomConnectError::Room(reason)),
        }
    }
}
