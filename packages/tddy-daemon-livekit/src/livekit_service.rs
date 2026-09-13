//! `livekit.LiveKitService`: what rooms this daemon can see on the LiveKit server, and who is in
//! them.
//!
//! Split out of `the pre-unbundle monolithic RPC coordinate` by `#unbundle` node 4. One method — leaving it
//! behind would keep the daemon serving a handler for a subsystem that lives in
//! `tddy-daemon-livekit`, which is the shape this stack exists to remove.
//!
//! The poll/diff arithmetic is [`crate::livekit_rooms_stream`]; this module is the handler's
//! contract with the transport — it authenticates before it opens a stream, and it stops reading
//! LiveKit once its subscriber is gone.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::livekit::{LiveKitRoomsEvent, LiveKitService, StreamLiveKitRoomsRequest};

use crate::livekit_rooms_stream::{pump_rooms, RoomRoster};

/// Cadence at which a `StreamLiveKitRooms` subscription re-reads the LiveKit roster. Presence is
/// the volatile fact on that panel, hence far shorter than the host-stats disk tick.
pub const LIVEKIT_ROOMS_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Stream adapter backed by an mpsc channel for [`LiveKitRoomsEvent`] server-streaming.
///
/// Carries results rather than events: a roster read that fails ends the stream with that error,
/// since an empty room list would read to the panel as "the server has no rooms".
#[derive(Debug)]
pub struct MpscLiveKitRoomsStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<LiveKitRoomsEvent, Status>>,
}

impl Stream for MpscLiveKitRoomsStream {
    type Item = Result<LiveKitRoomsEvent, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// Serves `livekit.LiveKitService`.
pub struct LiveKitServiceImpl {
    /// Reader for the LiveKit server's rooms and their participants.
    room_roster: Arc<dyn RoomRoster>,
    /// Cadence at which a subscription re-reads the roster (overridable for tests).
    poll_interval: Duration,
    /// Resolves a `session_token` to a GitHub login, or refuses it. The daemon's identity boundary
    /// is `tddy-daemon-auth`; this service is handed the resolver rather than reaching for it, so
    /// the LiveKit surface never learns how a token is verified.
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
}

impl LiveKitServiceImpl {
    pub fn new(
        room_roster: Arc<dyn RoomRoster>,
        user_resolver: tddy_daemon_kernel::SessionUserResolver,
    ) -> Self {
        Self {
            room_roster,
            poll_interval: LIVEKIT_ROOMS_POLL_INTERVAL,
            user_resolver,
        }
    }

    /// Override the poll cadence — lets a test observe several ticks inside its own timeout.
    #[must_use]
    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }
}

#[async_trait]
impl LiveKitService for LiveKitServiceImpl {
    type StreamLiveKitRoomsStream = MpscLiveKitRoomsStream;

    /// Stream the LiveKit server's rooms and their participants: one full snapshot, then one change
    /// event per delta found by polling the room service.
    ///
    /// Authenticates `session_token`, then spawns [`pump_rooms`], which emits the snapshot
    /// immediately and re-reads the roster on the poll cadence, diffing each read against the state
    /// **this** stream was last sent — a per-subscriber baseline, so two watchers cannot consume
    /// each other's deltas. A tick with no delta emits nothing, so an idle server yields an idle
    /// stream. The task ends when the receiver is dropped (client unsubscribe), and a roster read
    /// that fails ends the stream with that error rather than reporting an empty server.
    async fn stream_live_kit_rooms(
        &self,
        request: Request<StreamLiveKitRoomsRequest>,
    ) -> Result<Response<Self::StreamLiveKitRoomsStream>, Status> {
        let req = request.into_inner();
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<LiveKitRoomsEvent, Status>>();
        tokio::spawn(pump_rooms(
            Arc::clone(&self.room_roster),
            self.poll_interval,
            tx,
        ));

        Ok(Response::new(MpscLiveKitRoomsStream { rx }))
    }
}

/// The `livekit.LiveKitService` entry the daemon's wiring layer registers — family T.
///
/// Returned assembled, the way `tddy_model_registry::build_model_registry_entry` is: this
/// subsystem's whole contract with the wiring layer is the entry, so the wiring never names the
/// service type or its poll cadence.
pub fn build_livekit_entry(
    room_roster: Arc<dyn RoomRoster>,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> tddy_rpc::ServiceEntry {
    let server = tddy_service::LiveKitServiceServer::new(LiveKitServiceImpl::new(
        room_roster,
        user_resolver,
    ));
    tddy_rpc::ServiceEntry {
        name: "livekit.LiveKitService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
