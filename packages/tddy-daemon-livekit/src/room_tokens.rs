//! LiveKit's [`RoomTokenMinter`], built once from the `livekit:` block.

use std::sync::Arc;

use tddy_daemon_extensions::RoomTokenMinter;
use tddy_daemon_kernel::config::LiveKitConfig;

/// The minter the `livekit:` block supports: a real one when `url`, `api_key` and `api_secret` are
/// all set; otherwise one whose every mint fails, naming the missing fields.
pub fn room_token_minter(livekit: Option<&LiveKitConfig>) -> Arc<dyn RoomTokenMinter> {
    let _ = livekit;
    todo!("room_token_minter")
}
