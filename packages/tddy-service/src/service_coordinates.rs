//! The service coordinates whose *served* name and *addressed* name live in different crates.
//!
//! A coordinate is a routing address: the string a host registers a [`tddy_rpc::ServiceEntry`]
//! under, and the string a caller puts on the wire to reach it. The two have to be byte-identical,
//! and nothing checks that they are — a mismatch is not a type error, it is a **runtime** "unknown
//! service" on the peer, reported at the call site as a failure of the method rather than of the
//! spelling. `#unbundle` node 6 was bitten by exactly that: forwarded session-file calls reached a
//! peer that did not serve the coordinate they were addressed at.
//!
//! So a coordinate whose two ends sit in different crates is published here, once, and both ends
//! consume it. This module lives in `tddy-service` for the same reason [`crate::tonic_status`]
//! does: every crate on either end of such a wire already depends on `tddy-service`, while it
//! depends on none of them — and for these coordinates it is also the crate that owns the `.proto`
//! the name is declared in.
//!
//! A coordinate served and addressed *inside one crate* does not belong here — it is published by
//! that crate beside the entry that serves it (`tddy_terminal_rpc::TERMINAL_SESSION_SERVICE`, for
//! the proto that crate owns).

/// The coordinate `session_files.proto`'s thirteen session-file methods are served at.
///
/// Served by `tddy_session_files::service::build_session_files_entry` (and by the daemon wrapper
/// that routes eight of the thirteen to a peer), addressed by `tddy-daemon-livekit`'s
/// session-file forwarders. `package session_files` + `service SessionFilesService`, which
/// `tests/service_coordinates.rs` pins against the schema itself.
pub const SESSION_FILES_SERVICE: &str = "session_files.SessionFilesService";
