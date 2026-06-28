//! macOS Seatbelt (`sandbox-exec`) sandbox implementation.

mod profile;
mod spawn;

pub use profile::render_plan;
pub use spawn::{detect_allow_read_paths, sandbox_exec_argv, spawn_plan};

// The in-jail runner + host relay are platform-agnostic and live in `tddy-sandbox-runner` (shared
// with the Linux cgroups backend, the daemon, the app, and tests). Re-exported here so existing
// importers of `tddy_sandbox_darwin::{run_sandbox_runner, SandboxRunnerArgs, connect_sandbox_client}`
// keep compiling unchanged.
pub use tddy_sandbox_runner::{
    connect_sandbox_client, connect_sandbox_client_uds, run_sandbox_runner, SandboxRunnerArgs,
};
