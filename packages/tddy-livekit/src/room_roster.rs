//! Reading the LiveKit server's own view of its rooms and who is joined to them.
//!
//! The dashboard's rooms panel answers "what exists on the server", which only the server API can
//! say — a client SDK sees the one room it joined. This is the production reader behind that panel:
//! `ListRooms`, then `ListParticipants` per room, mapped into the wire types the panel renders.
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`

use crate::server_api_url::{http_base_from_ws_url, ServerApiUrlError};
use livekit_api::services::room::RoomClient;
use livekit_protocol::ParticipantInfo;
use std::time::Duration;
use tddy_service::proto::connection::{LiveKitParticipantInfo, LiveKitRoomInfo};

/// Ceiling on one whole roster read — `ListRooms` plus a `ListParticipants` per room.
///
/// The rooms feed re-reads every three seconds, so a read still unanswered after five is already
/// staler than the data the next tick would carry; failing it hands the subscriber an error instead
/// of holding the stream on a roster that looks healthy while the server API is hung. Five seconds
/// is far above a healthy read (single-digit milliseconds against a LAN deployment), so ordinary
/// slowness is not reported as breakage. Bounding the whole read rather than each call keeps the
/// ceiling independent of how many rooms the server happens to hold.
const ROSTER_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Lists the rooms of one LiveKit deployment over its server API.
pub struct LiveKitRoomRoster {
    client: RoomClient,
    read_timeout: Duration,
}

impl LiveKitRoomRoster {
    /// Address the server API of the deployment whose signalling URL is `ws_url` — the daemon's own
    /// `livekit.url`, not the browser-facing `public_url`, since the daemon reaches the server over
    /// its own (possibly internal) address.
    pub fn from_ws_url(
        ws_url: &str,
        api_key: &str,
        api_secret: &str,
    ) -> Result<Self, ServerApiUrlError> {
        let http_base = http_base_from_ws_url(ws_url)?;
        Ok(Self {
            client: RoomClient::with_api_key(&http_base, api_key, api_secret),
            read_timeout: ROSTER_READ_TIMEOUT,
        })
    }

    /// Bound a roster read by `read_timeout` instead of the default [`ROSTER_READ_TIMEOUT`].
    pub fn with_read_timeout(mut self, read_timeout: Duration) -> Self {
        self.read_timeout = read_timeout;
        self
    }

    /// Every room on the server with its participants, in the order the server reports them.
    ///
    /// One `ListParticipants` call per room, so the cost is proportional to the room count. An
    /// unreachable server yields the error rather than an empty list — "no rooms" is a fact about
    /// the server, not about the connection to it — and so does one that accepts the read and never
    /// answers it, once [`ROSTER_READ_TIMEOUT`] has passed.
    pub async fn list_rooms(&self) -> Result<Vec<LiveKitRoomInfo>, String> {
        tokio::time::timeout(self.read_timeout, self.read_roster())
            .await
            .map_err(|_| {
                format!(
                    "livekit roster read timed out after {:?}",
                    self.read_timeout
                )
            })?
    }

    /// The roster read itself, without its deadline.
    async fn read_roster(&self) -> Result<Vec<LiveKitRoomInfo>, String> {
        let rooms = self
            .client
            .list_rooms(vec![])
            .await
            .map_err(|err| format!("livekit ListRooms failed: {err}"))?;

        let mut roster = Vec::with_capacity(rooms.len());
        for room in rooms {
            let joined = self
                .client
                .list_participants(&room.name)
                .await
                .map_err(|err| format!("livekit ListParticipants({}) failed: {err}", room.name))?;
            roster.push(LiveKitRoomInfo {
                name: room.name,
                // `creation_time` is seconds and has been in the protocol since the beginning, so
                // it reads correctly against any server version; the panel renders a wall-clock
                // time, for which sub-second precision is not a fact it displays.
                created_at_ms: room.creation_time * 1_000,
                participants: joined.iter().map(participant_info).collect(),
                metadata: room.metadata,
            });
        }
        Ok(roster)
    }
}

/// One participant as the server API reports it, in the wire type the rooms panel renders.
fn participant_info(p: &ParticipantInfo) -> LiveKitParticipantInfo {
    LiveKitParticipantInfo {
        identity: p.identity.clone(),
        name: p.name.clone(),
        // Relayed verbatim: the daemon neither parses nor validates what publishers put here.
        metadata: p.metadata.clone(),
        // The server reports `joined_at = 0` for a participant whose join time it has not recorded
        // yet — one still `JOINING`. That zero is carried through as the "no join time recorded"
        // sentinel the proto documents, rather than being turned into a time this crate would have
        // invented.
        joined_at_ms: p.joined_at * 1_000,
        // "JOINING" / "JOINED" / "ACTIVE" / "DISCONNECTED", named by the protocol itself rather
        // than by a mapping this crate would have to keep in step.
        state: p.state().as_str_name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_protocol::participant_info::State;

    /// A participant as the server API reports one, with every field the mapping reads set to a
    /// plausible value so a bare `a_participant()` maps to a complete row.
    fn a_participant() -> ParticipantBuilder {
        ParticipantBuilder(ParticipantInfo {
            identity: "daemon-alpha".to_string(),
            name: "Alpha daemon".to_string(),
            metadata: r#"{"role":"daemon"}"#.to_string(),
            joined_at: 1_764_072_000,
            state: State::Active as i32,
            ..ParticipantInfo::default()
        })
    }

    struct ParticipantBuilder(ParticipantInfo);

    impl ParticipantBuilder {
        fn with_identity(mut self, identity: &str) -> Self {
            self.0.identity = identity.to_string();
            self
        }

        fn with_metadata(mut self, metadata: &str) -> Self {
            self.0.metadata = metadata.to_string();
            self
        }

        fn joined_at_seconds(mut self, joined_at: i64) -> Self {
            self.0.joined_at = joined_at;
            self
        }

        fn with_no_recorded_join_time(mut self) -> Self {
            self.0.joined_at = 0;
            self
        }

        fn in_state(mut self, state: State) -> Self {
            self.0.state = state as i32;
            self
        }

        fn build(self) -> ParticipantInfo {
            self.0
        }
    }

    #[test]
    fn relays_the_identity_the_server_reported_verbatim() {
        // Given a participant the server names by its identity
        let joined = a_participant().with_identity("daemon-beta").build();

        // When it is mapped for the rooms panel
        let mapped = participant_info(&joined);

        // Then the identity crosses unchanged
        assert_eq!(mapped.identity, "daemon-beta");
    }

    #[test]
    fn relays_the_published_metadata_verbatim() {
        // Given a participant publishing metadata this crate neither parses nor validates
        let joined = a_participant()
            .with_metadata(r#"{"owned_project_count":3}"#)
            .build();

        // When it is mapped for the rooms panel
        let mapped = participant_info(&joined);

        // Then the document crosses byte for byte
        assert_eq!(mapped.metadata, r#"{"owned_project_count":3}"#);
    }

    #[test]
    fn reports_a_recorded_join_time_in_milliseconds() {
        // Given a participant whose join time the server recorded, in seconds
        let joined = a_participant().joined_at_seconds(1_764_072_000).build();

        // When it is mapped for the rooms panel
        let mapped = participant_info(&joined);

        // Then the panel receives the same instant in milliseconds
        assert_eq!(mapped.joined_at_ms, 1_764_072_000_000);
    }

    #[test]
    fn keeps_an_unrecorded_join_time_as_the_zero_sentinel() {
        // Given a participant still joining, for which the server has recorded no join time
        let joined = a_participant()
            .with_no_recorded_join_time()
            .in_state(State::Joining)
            .build();

        // When it is mapped for the rooms panel
        let mapped = participant_info(&joined);

        // Then the zero is carried through as the documented "not known" sentinel, not scaled into
        // a 1970 timestamp
        assert_eq!(mapped.joined_at_ms, 0);
    }

    #[test]
    fn names_the_participant_state_as_the_protocol_names_it() {
        // Given a participant the server reports as disconnected
        let joined = a_participant().in_state(State::Disconnected).build();

        // When it is mapped for the rooms panel
        let mapped = participant_info(&joined);

        // Then the state is the protocol's own name for it
        assert_eq!(mapped.state, "DISCONNECTED");
    }

    #[test]
    fn refuses_a_configured_url_that_is_not_a_websocket_address() {
        // Given a daemon configured with the HTTP address by mistake
        let url = "http://127.0.0.1:7880";

        // When a roster is built for it
        let roster = LiveKitRoomRoster::from_ws_url(url, "devkey", "secret");

        // Then construction fails naming the offending value, rather than yielding a client that
        // would report "no rooms" against an address it can never reach
        assert_eq!(
            roster.err(),
            Some(ServerApiUrlError::NotWebSocketScheme(url.to_string()))
        );
    }

    #[test]
    fn accepts_the_signalling_url_the_daemon_is_configured_with() {
        // Given the daemon's own LiveKit signalling address
        let url = "ws://127.0.0.1:7880";

        // When a roster is built for it
        let roster = LiveKitRoomRoster::from_ws_url(url, "devkey", "secret");

        // Then it is ready to read the server API
        assert!(roster.is_ok(), "expected a roster for {url}");
    }
}
