//! Muse Code (Meta's `muse` CLI) as a CLI harness: its descriptor, its curated models and the
//! variable its key is handed in.
//!
//! Only the interactive TUI is integrated. Muse Code's headless protocols (`muse exec --json`,
//! `muse serve`) are undocumented; see the backlog.

use crate::backend::{BackendModel, BackendModels};
use crate::cli_harness::{CliHarness, CliLaunch, CliModelSource};
use crate::provider_route::Harness;

/// The variable Muse Code reads a Model API key from for non-interactive use. Secondary sources
/// (2026-10) name `META_API_KEY`; `/green` confirms it against `muse --help` before relying on it.
pub const MUSE_ROUTE_KEY_ENV: &str = "META_API_KEY";

/// The Muse models offered when no Meta account lists its own.
#[must_use]
pub fn muse_cli_models() -> BackendModels {
    BackendModels {
        models: vec![
            BackendModel::new("muse-spark-1.3", "Muse Spark 1.3"),
            BackendModel::new("muse-wonder-1.0", "Muse Wonder 1.0"),
        ],
        default_model: "muse-spark-1.3".to_string(),
    }
}

/// `muse [--model <m>] [<prompt>]` to start; `muse --continue` to resume the worktree's session,
/// which is this one (one session per worktree). A blank first prompt is no prompt.
fn muse_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let mut argv = vec![launch.binary.to_string()];
    if launch.resume {
        argv.push("--continue".to_string());
        return argv;
    }
    if !launch.model.is_empty() {
        argv.push("--model".to_string());
        argv.push(launch.model.to_string());
    }
    if let Some(prompt) = launch
        .initial_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        argv.push(prompt.to_string());
    }
    argv
}

/// The Muse Code descriptor, registered in [`crate::cli_harness::cli_harnesses`]. A `const`, not a
/// `static`: the registry is an array of descriptor values, and a `static`'s value cannot be copied
/// into one.
pub const MUSE_CLI: CliHarness = CliHarness {
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
