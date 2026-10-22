//! The in-jail PTY for the CLI harnesses beyond Claude Code and Cursor — `--agent-kind opencode |
//! muse`. The argv comes from the same descriptors the host uses
//! (`tddy_agent_backend::cli_harness`), so a jailed session starts exactly as an unsandboxed one
//! does. The PTY is the one the Claude and Cursor paths already run, so the agent child receives
//! the resolved `TDDY_SECRET_*` envs the same way.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use tddy_agent_backend::cli_harness::{cli_harness, CliHarness, CliLaunch};

use crate::runner::{
    boot_log_error, run_claude_pty_thread, write_failure_marker, PtyState, SandboxSessionRelay,
};

/// The CLI harness `--agent-kind <kind>` hosts, by its descriptor; `Err` for a kind the runner does
/// not host.
fn hosted_harness(agent_kind: &str) -> Result<&'static CliHarness, String> {
    let session_type = match agent_kind {
        "opencode" => "opencode-cli",
        "muse" => "muse-cli",
        other => {
            return Err(format!(
                "--agent-kind {other:?} is not hosted in the runner's PTY; expected \"opencode\" or \"muse\""
            ))
        }
    };
    cli_harness(session_type)
        .ok_or_else(|| format!("no CLI harness is registered for {session_type}"))
}

/// The argv `--agent-kind <kind>` runs in the PTY; `Err` for a kind the runner does not host.
pub fn harness_pty_argv(
    agent_kind: &str,
    binary: &str,
    model: &str,
    initial_prompt: Option<&str>,
    resume: bool,
) -> Result<Vec<String>, String> {
    let harness = hosted_harness(agent_kind)?;
    Ok((harness.argv)(&CliLaunch {
        binary,
        model,
        initial_prompt,
        resume,
        ..CliLaunch::default()
    }))
}

/// The binary `--agent-kind <kind>` runs: `--agent-binary` when one is given, else the descriptor's.
pub fn harness_pty_binary(agent_kind: &str, requested: Option<&str>) -> Result<String, String> {
    let harness = hosted_harness(agent_kind)?;
    Ok(requested.unwrap_or(harness.default_binary).to_string())
}

/// What a `--agent-kind opencode | muse` PTY is started with.
pub(crate) struct SpawnHarnessPtyParams<'a> {
    pub(crate) agent_kind: &'a str,
    /// `--agent-binary`, when one is given.
    pub(crate) agent_binary: Option<&'a str>,
    pub(crate) model: &'a str,
    /// Resume the harness's own last session rather than start one.
    pub(crate) resume: bool,
    pub(crate) cwd: &'a Path,
    pub(crate) egress_shim: &'a str,
    pub(crate) relay: Arc<SandboxSessionRelay>,
    pub(crate) initial_cols: u16,
    pub(crate) initial_rows: u16,
}

/// Start the harness in the runner's PTY. The PTY thread resolves the `TDDY_SECRET_*` envs from the
/// runner's own environment and sets them on the agent child only, as it does for Claude and Cursor.
pub(crate) fn spawn_harness_pty(params: SpawnHarnessPtyParams<'_>) -> Result<PtyState> {
    let binary =
        harness_pty_binary(params.agent_kind, params.agent_binary).map_err(anyhow::Error::msg)?;
    // No first prompt is passed: the runner has no input for one yet.
    let argv = harness_pty_argv(
        params.agent_kind,
        &binary,
        params.model,
        None,
        params.resume,
    )
    .map_err(anyhow::Error::msg)?;
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Bytes>();
    let cwd = params.cwd.to_path_buf();
    let egress_shim = params.egress_shim.to_string();
    let relay = params.relay;
    let (initial_cols, initial_rows) = (params.initial_cols, params.initial_rows);

    std::thread::spawn(move || {
        if let Err(e) = run_claude_pty_thread(
            argv,
            cwd,
            egress_shim,
            relay,
            stdin_rx,
            initial_cols,
            initial_rows,
        ) {
            boot_log_error("spawn_harness_pty", format!("{e:#}"));
            write_failure_marker(&format!("spawn_harness_pty failed: {e:#}"));
        }
    });

    Ok(PtyState { stdin_tx })
}
