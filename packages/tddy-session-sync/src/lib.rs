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
pub mod attach;
pub mod credentials;
pub mod mirror;
pub mod sync;

pub use crate::attach::daemon_http::DaemonHttpError;
pub use crate::sync::delta::decide_record;
pub use crate::sync::delta::decide_worktree;
pub use crate::sync::delta::delta_request;
pub use crate::sync::delta::reassemble;
pub use crate::sync::delta::DeltaError;
pub use crate::sync::delta::IgnoreReason;
pub use crate::sync::delta::RecordDecision;
pub use crate::sync::delta::WorktreeDecision;
pub use crate::sync::git::first_attach_commands;
pub use crate::sync::git::reconcile_commands;
pub use crate::sync::git::remote_url;
pub use crate::sync::git::wip_ref;
pub use crate::sync::git::GitInvocation;
pub use crate::sync::git::GitTransport;
pub use apply::{ApplyOutcome, Delta, ReconcileReason};
pub use attach::{
    attach, daemon_identity, join_session_room, open_session_room, resolve_session,
    session_room_name, syncer_identity, AttachError, AttachedSession, SessionAddress,
};
pub use credentials::{
    layered_environment, parse_env_file, resolve_credentials, CredentialError, Credentials,
    DaemonToken, LiveKitCredentials, SyncArgs,
};
pub use mirror::{Mirror, MirrorError, MirrorMarker, MARKER_FILENAME};
pub use sync::{
    run, SyncError, LOCAL_WIP_REF, MIRROR_DELTA_SCOPE, REMOTE_GIT_SSH_COMMAND, STREAM_DELTA_METHOD,
};
