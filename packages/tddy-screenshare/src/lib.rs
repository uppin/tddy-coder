pub mod bridge;
pub mod client;
pub mod common;
pub mod streamer;

pub use bridge::{run_bridge, BridgeConfig};
pub use client::{Credentials, ScreenSharingClient};
pub use tddy_daemon_extensions::bridge_readiness::{
    read_readiness, write_readiness, AuthenticationRefused, BridgeReadiness, FailureKind,
};
