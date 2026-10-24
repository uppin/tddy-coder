//! Warp's Agent CLI (the `warp` binary) as a CLI harness: its descriptor, its argument list and the
//! reader of the resume token it prints on exit.
//!
//! `warp` takes no model and no first prompt on its command line — both are chosen inside the TUI
//! (`/model`, `/agent <prompt>`). A conversation is reopened with `warp --resume <TOKEN>`, and the
//! token exists only once a run ends: Warp prints the command to resume with as it exits. The daemon
//! reads it back off the terminal with [`resume_token_from_output`] and hands it to the next launch
//! as [`CliLaunch::conversation_id`].

use crate::backend::BackendModels;
use crate::cli_harness::{CliHarness, CliLaunch, CliModelSource};
use crate::provider_route::Harness;

/// Warp offers no model to choose from outside its TUI.
#[must_use]
pub fn warp_cli_models() -> BackendModels {
    // TODO(warp-cli-session): implement
    todo!("warp_cli_models")
}

/// `warp` to start; `warp --resume <token>` to resume a conversation whose token was recorded. A
/// resume with no token starts a fresh conversation — never a bare `--resume`.
fn warp_argv(_launch: &CliLaunch<'_>) -> Vec<String> {
    // TODO(warp-cli-session): implement
    todo!("warp_argv")
}

/// The token of the last `warp --resume <TOKEN>` line in `output`, a terminal's raw bytes with
/// their escape sequences; `None` when the run printed none.
#[must_use]
pub fn resume_token_from_output(_output: &[u8]) -> Option<String> {
    // TODO(warp-cli-session): implement
    todo!("resume_token_from_output")
}

/// The Warp descriptor, registered in [`crate::cli_harness::cli_harnesses`].
pub const WARP_CLI: CliHarness = CliHarness {
    session_type: "warp-cli",
    label: "Warp",
    route_harness: Harness::WarpCli,
    context_agent: "warp",
    default_binary: "warp",
    binary_env: "TDDY_WARP_CLI",
    route_config_env: None,
    argv: warp_argv,
    models: CliModelSource::Curated(warp_cli_models),
};
