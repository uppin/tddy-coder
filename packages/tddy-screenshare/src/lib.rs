pub mod bridge;
pub mod client;
pub mod common;
pub mod inspect;
pub mod streamer;

pub use bridge::{run_bridge, BridgeConfig};
pub use client::{Credentials, ScreenSharingClient};
pub use inspect::{
    read_inspect_report, run_inspect, write_inspect_report, AdvertisedMethod, BridgeMode,
    InspectReport, InspectTarget, INSPECT_FLAG,
};
pub use tddy_daemon_extensions::bridge_readiness::{
    read_readiness, write_readiness, AuthenticationRefused, BridgeReadiness, FailureKind,
};
