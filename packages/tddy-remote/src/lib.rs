//! Library surface for `tddy-remote` (CLI delegates here).

mod connect_client;

pub mod config;
pub mod rsh;
pub mod session;
pub mod vfs_path;

pub use config::{load_authority_ids_from_path, load_authority_ids_from_yaml, RemoteConfigError};
pub use rsh::run_rsync_rsh;
pub use session::{run_exec, run_shell_pty};
pub use vfs_path::normalize_sandbox_relative_path;
