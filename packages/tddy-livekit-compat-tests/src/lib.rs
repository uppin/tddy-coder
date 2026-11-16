//! LiveKit connectivity and contract suites.
//!
//! This crate holds no product logic. Every suite in `tests/` proves one thing: that the LiveKit
//! adapter of a port (the RPC transport in `tddy-livekit`, the common-room and session-room
//! surfaces in `tddy-daemon-livekit`, a spawned participant's serving identity) connects to a real
//! LiveKit server and honours the same contract the cheap in-memory, stdio and IPC adapters honour
//! in the crates that own the behaviour.
//!
//! A suite that asserts a product use-case belongs in the crate that owns that use-case, running
//! over a cheap transport. `tests/livekit_test_placement.rs` pins that split: it names the suites
//! that live here, the crates they left, and the logic suites still paying for a real room.
