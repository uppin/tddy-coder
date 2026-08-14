//! Room metadata as a snapshot every joiner reads on arrival.
//!
//! A broadcast (see [`crate::broadcast`]) only reaches whoever is connected at publish time, so it
//! cannot tell a participant that joins later where things stand. Room metadata covers exactly that
//! gap: the server hands the current value to each participant as part of the join, with no history
//! to replay.
//!
//! Writing it goes through LiveKit's Twirp server API rather than the room connection, because
//! room metadata is a room-admin operation and a participant's token does not grant it.
//! [`livekit_api::services::room::RoomClient`] signs its **own** room-admin token from the api
//! key/secret on every call — which is why nothing in [`crate::token`] needs a `room_admin` grant,
//! and why this client needs the api secret rather than a participant token.

use anyhow::{Context, Result};
use livekit_api::services::room::{CreateRoomOptions, RoomClient};

/// Derive the Twirp server-API base URL from the signalling URL configured for participants.
///
/// Both live at the same host and port; only the scheme differs. Anything that is not a WebSocket
/// URL passes through untouched, so a caller that already holds an `http(s)` base can pass it in.
pub fn livekit_http_url(ws_url: &str) -> String {
    if let Some(rest) = ws_url.strip_prefix("wss://") {
        return format!("https://{rest}");
    }
    if let Some(rest) = ws_url.strip_prefix("ws://") {
        return format!("http://{rest}");
    }
    ws_url.to_string()
}

/// Writes room metadata through the LiveKit server API.
pub struct RoomMetadataClient {
    rooms: RoomClient,
}

impl RoomMetadataClient {
    /// Build a client for the LiveKit deployment reachable at `url`, which may be given as either
    /// the participant `ws(s)://` URL or the `http(s)://` server-API URL.
    pub fn with_api_key(url: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            rooms: RoomClient::with_api_key(&livekit_http_url(url), api_key, api_secret),
        }
    }

    /// Ensure the room exists and carries `metadata`, whether or not it existed before.
    ///
    /// This is two calls on purpose. `create_room` against an existing room succeeds by returning
    /// the room it found, *without* applying the options it was given — so a bare create would
    /// silently leave a re-created room showing whatever metadata it had from its first creation.
    /// Creating first and then setting the metadata makes the post-condition the same on both
    /// paths, which is what lets a daemon call this on every session start.
    pub async fn create_with_metadata(&self, room: &str, metadata: &str) -> Result<()> {
        self.rooms
            .create_room(
                room,
                CreateRoomOptions {
                    metadata: metadata.to_string(),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("creating LiveKit room {room}"))?;
        self.set_metadata(room, metadata).await
    }

    /// Replace the room's metadata. Last write wins — there is no merge, so this is only correct
    /// while a single writer owns the room's metadata.
    pub async fn set_metadata(&self, room: &str, metadata: &str) -> Result<()> {
        self.rooms
            .update_room_metadata(room, metadata)
            .await
            .with_context(|| format!("updating metadata of LiveKit room {room}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_insecure_websocket_url_becomes_a_plain_http_url() {
        // Given the signalling URL of a local, unencrypted deployment
        let ws_url = "ws://127.0.0.1:7880";

        // When the server-API URL is derived from it
        let http_url = livekit_http_url(ws_url);

        // Then only the scheme changed
        assert_eq!(http_url, "http://127.0.0.1:7880");
    }

    #[test]
    fn a_secure_websocket_url_becomes_an_https_url() {
        // Given the signalling URL of a TLS deployment
        let ws_url = "wss://livekit.example.com";

        // When the server-API URL is derived from it
        let http_url = livekit_http_url(ws_url);

        // Then the derived URL is TLS too — downgrading it would send the api secret in the clear
        assert_eq!(http_url, "https://livekit.example.com");
    }

    #[test]
    fn a_url_that_is_already_http_passes_through_unchanged() {
        // Given a caller that configured the server-API URL directly
        let configured = "https://livekit.example.com/api";

        // When it is normalized
        let http_url = livekit_http_url(configured);

        // Then it is used as-is
        assert_eq!(http_url, "https://livekit.example.com/api");
    }
}
