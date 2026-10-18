pub mod bridge;
pub mod client;
pub mod common;
pub mod readiness;
pub mod streamer;

pub use bridge::{run_bridge, BridgeConfig};
pub use client::{Credentials, ScreenSharingClient};
pub use readiness::{
    read_readiness, write_readiness, AuthenticationRefused, BridgeReadiness, FailureKind,
};
