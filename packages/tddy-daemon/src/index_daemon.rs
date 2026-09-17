//! Lazy get-or-spawn, supervision, restart and idle stop of the one `tddy-index-daemon` process
//! this daemon manages.
//!
//! The shape is [`tddy_lsp::LspRegistry::get_or_spawn`] one level up, and for the same reason its
//! own documentation gives: [`tddy_task::TaskRegistry`] owns *process lifetime* and the
//! SIGTERM→SIGKILL escalation net, but it keys by generated id and evicts terminal tasks, so
//! "the process we already started" needs a layer that keys by identity. Here the identity is
//! trivial — there is exactly **one** index daemon per host, because that process is itself
//! root-parameterised and serves every workspace root
//! (`docs/ft/coder/warm-code-intelligence-daemon.md` § *One process, many
//! worktrees*) — so the map collapses to a single slot.
//!
//! What the layer still has to do is everything the trivial key does not remove: start the process
//! only when something needs it, hand the same process to the next caller, notice that it has
//! exited and replace it rather than return a dead handle, stop it once it has gone unused, and
//! stop it when this daemon shuts down.

mod error;
mod registry;
mod spawn;

pub use error::IndexDaemonError;
pub use registry::IndexDaemonRegistry;
pub use spawn::{resolve_index_daemon_path, IndexDaemon, IndexDaemonSpawn};
