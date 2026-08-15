//! Mirror a tddy session's worktree locally by watching its LiveKit room.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md`.
//!
//! The session room publishes what the agent did (`session.activity`) and when the checkout moved
//! (`worktree.activity`); the daemon serves the patch behind each of those and a byte-exact read of
//! any worktree file. This crate consumes all three and keeps a directory equal to the session's
//! worktree — committed history over the git transport that already exists, uncommitted edits over
//! the deltas this feature adds.
//!
//! Split the way `tddy-remote-git-repo` is, and for the same reason: everything that can be decided
//! without I/O is a pure function over injected inputs, so it is testable without a daemon, a room
//! or a clock.

pub mod apply;
pub mod credentials;
pub mod mirror;

pub use apply::{ApplyOutcome, Delta, ReconcileReason};
pub use credentials::{
    layered_environment, parse_env_file, resolve_credentials, CredentialError, Credentials,
    DaemonToken, LiveKitCredentials, SyncArgs,
};
pub use mirror::{Mirror, MirrorError, MirrorMarker, MARKER_FILENAME};
