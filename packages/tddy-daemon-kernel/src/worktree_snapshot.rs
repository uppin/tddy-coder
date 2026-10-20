//! What one poll of a checkout observed, as the room and the agent roster both read it.
//!
//! Defined in `tddy_daemon_extensions::split_rooms`, which owns the split-placement port that returns
//! it; re-exported here so this module's path is unchanged.

pub use tddy_daemon_extensions::split_rooms::WorktreeSnapshot;
