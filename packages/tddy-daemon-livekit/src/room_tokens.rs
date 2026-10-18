//! LiveKit's [`RoomTokenMinter`], built once from the `livekit:` block.

use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_extensions::RoomTokenMinter;
use tddy_daemon_kernel::config::LiveKitConfig;
use tddy_livekit::TokenGenerator;
use tddy_rpc::Status;

/// The minter the `livekit:` block supports: a real one when `url`, `api_key` and `api_secret` are
/// all set; otherwise one whose every mint fails, naming the missing fields.
pub fn room_token_minter(livekit: Option<&LiveKitConfig>) -> Arc<dyn RoomTokenMinter> {
    let Some(block) = livekit else {
        return Arc::new(tddy_daemon_extensions::NoRoomTokens);
    };
    let missing: Vec<&'static str> = [
        ("url", block.url.is_none()),
        ("api_key", block.api_key.is_none()),
        ("api_secret", block.api_secret.is_none()),
    ]
    .into_iter()
    .filter_map(|(field, absent)| absent.then_some(field))
    .collect();
    match (&block.api_key, &block.api_secret) {
        (Some(api_key), Some(api_secret)) if missing.is_empty() => Arc::new(ConfiguredMinter {
            api_key: api_key.clone(),
            api_secret: api_secret.clone(),
        }),
        _ => Arc::new(IncompleteBlock { missing }),
    }
}

/// Signs room tokens with the configured key and secret.
struct ConfiguredMinter {
    api_key: String,
    api_secret: String,
}

impl RoomTokenMinter for ConfiguredMinter {
    fn mint(&self, room: &str, identity: &str, ttl: Duration) -> Result<String, Status> {
        TokenGenerator::new(
            self.api_key.clone(),
            self.api_secret.clone(),
            room.to_string(),
            identity.to_string(),
            ttl,
        )
        .generate()
        .map_err(|e| Status::internal(format!("signing the room token for {room} failed: {e}")))
    }
}

/// A `livekit:` block that cannot mint: every mint names what it lacks.
struct IncompleteBlock {
    missing: Vec<&'static str>,
}

impl RoomTokenMinter for IncompleteBlock {
    fn mint(&self, room: &str, _identity: &str, _ttl: Duration) -> Result<String, Status> {
        Err(Status::failed_precondition(format!(
            "this daemon's livekit block is missing {}, so it can mint no room token for {room}",
            self.missing.join(", ")
        )))
    }
}
