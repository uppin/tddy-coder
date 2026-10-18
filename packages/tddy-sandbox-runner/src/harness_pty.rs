//! The in-jail PTY argv for the CLI harnesses beyond Claude Code and Cursor — `--agent-kind
//! opencode | muse` — built from the same descriptors the host uses (`tddy_core::cli_harness`), so a
//! jailed session starts exactly as an unsandboxed one does.

/// The argv `--agent-kind <kind>` runs in the PTY; `Err` for a kind the runner does not host.
pub fn harness_pty_argv(
    agent_kind: &str,
    binary: &str,
    model: &str,
    initial_prompt: Option<&str>,
    resume: bool,
) -> Result<Vec<String>, String> {
    let _ = (agent_kind, binary, model, initial_prompt, resume);
    // TODO(sandboxed-routes): implement
    todo!()
}
