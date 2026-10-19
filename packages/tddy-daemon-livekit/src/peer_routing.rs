//! Peer routing moved to [`tddy_host_service::peer_routing`], which routes for every daemon and
//! names no LiveKit crate. Re-exported here so the session crates that name this path keep
//! compiling until node 11 moves them.

pub use tddy_host_service::peer_routing::PeerRouting;
