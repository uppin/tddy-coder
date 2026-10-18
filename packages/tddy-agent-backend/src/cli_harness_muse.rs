//! Muse Code (Meta's `muse` CLI) as a CLI harness: its descriptor, its curated models and the
//! variable its key is handed in.
//!
//! Only the interactive TUI is integrated. Muse Code's headless protocols (`muse exec --json`,
//! `muse serve`) are undocumented; see the backlog.

use crate::backend::BackendModels;
use crate::cli_harness::{CliHarness, CliLaunch, CliModelSource};
use crate::provider_route::Harness;

/// The variable Muse Code reads a Model API key from for non-interactive use. Secondary sources
/// (2026-10) name `META_API_KEY`; `/green` confirms it against `muse --help` before relying on it.
pub const MUSE_ROUTE_KEY_ENV: &str = "META_API_KEY";

/// The Muse models offered when no Meta account lists its own.
#[must_use]
pub fn muse_cli_models() -> BackendModels {
    // TODO(muse-cli): implement
    todo!()
}

/// `muse [--model <m>] [<prompt>]` to start; `muse --continue` to resume the worktree's session.
fn muse_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let _ = launch;
    // TODO(muse-cli): implement
    todo!()
}

/// The Muse Code descriptor, registered in [`crate::cli_harness::cli_harnesses`].
pub static MUSE_CLI: CliHarness = CliHarness {
    session_type: "muse-cli",
    label: "Muse Code",
    route_harness: Harness::MuseCli,
    context_agent: "muse",
    default_binary: "muse",
    binary_env: "TDDY_MUSE_CLI",
    route_config_env: None,
    argv: muse_argv,
    models: CliModelSource::Curated(muse_cli_models),
};
