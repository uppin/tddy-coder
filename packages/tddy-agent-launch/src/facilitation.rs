//! Which sessions this daemon facilitates — runs the agent of, and so hosts the room of — over a
//! checkout it holds. Re-exported from `session_coordinate_handlers`, where `ConnectSession` applies it.

use std::path::PathBuf;

/// What a session's metadata says about this daemon opening a room for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FacilitatedCheckout {
    /// The session's agent runs elsewhere, so its room is over there; opening one here would put
    /// it on a daemon that serves nobody.
    NotFacilitated,
    /// Facilitated here, yet recording no checkout on this daemon.
    NoCheckout,
    /// Facilitated here over this checkout: the only sessions this daemon opens a room for.
    Checkout(PathBuf),
}

/// Whether a session of this type is *facilitated* by the daemon it was started on — that is,
/// whether it runs an agent here and therefore has a session room of its own.
///
/// The PRD's Roles table as a predicate (`docs/ft/daemon/session-room.md` § Roles): the room
/// belongs to the daemon running the session's agent. `claude-cli`, `cursor-cli` and `opencode-cli` run one here.
/// A `workspace` session runs none — it is the codebase half of a split session or a standalone
/// checkout, and whatever agent it has is on another daemon. A `tool` (tddy-coder) session's
/// terminal is a room `tddy-coder` joins for itself, which this daemon neither creates nor serves.
///
/// Deliberately **not** a statement about whether a session type "uses LiveKit". A `claude-cli`
/// session also bridges its PTY into the lobby, and `ConnectSession` answers about a *terminal*
/// room none of these types has. Three different rooms; this names one of them.
pub fn session_type_is_facilitated_here(session_type: &str) -> bool {
    matches!(
        session_type.trim(),
        "claude-cli" | "cursor-cli" | "opencode-cli"
    )
}

/// The one rule for "a session this daemon facilitates over a checkout it holds", shared by
/// `ConnectSession` and an extension's own RPC.
pub fn facilitated_checkout(metadata: &tddy_core::SessionMetadata) -> FacilitatedCheckout {
    let session_type = metadata.session_type.as_deref().unwrap_or_default();
    if !session_type_is_facilitated_here(session_type) {
        return FacilitatedCheckout::NotFacilitated;
    }
    match metadata.repo_path.as_deref() {
        Some(worktree_root) => FacilitatedCheckout::Checkout(PathBuf::from(worktree_root)),
        None => FacilitatedCheckout::NoCheckout,
    }
}
