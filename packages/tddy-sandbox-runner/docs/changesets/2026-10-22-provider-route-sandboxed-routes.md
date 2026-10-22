# 2026-10-22 — OpenCode and Muse in the runner's PTY; the jail acknowledges an attach and lets go of a finished channel

**Type:** Feature + Fix

- **`--agent-kind opencode | muse`** (`harness_pty.rs`): `harness_pty_argv` builds the in-jail argv
  from the CLI harness descriptors (`tddy_agent_backend::cli_harness`), and `spawn_harness_pty` runs
  it in the Claude PTY thread, so the agent child gets the resolved `TDDY_SECRET_*` envs. Both refuse
  a blank `--model`. `--resume` continues the harness's last session, and an unhosted kind is refused.
- **Fix — attach acknowledgement**: a `SubscribeTerminal` for the runner's session is answered with
  one empty `TerminalOutput` frame (`acknowledge_attach`). Before, a PTY-less jail
  (`--workspace-tools`) never sent a first frame, and the host's ten-second attach wait timed out.
- **Fix — channel release**: the host relay's reader holds a `WeakSender` (`send_if_open`). The
  runner drops its captured outbound sender when the host closes its half (`release_outbound`, compared
  by channel). Before, either held sender kept the stream open, and a runner whose PTY command had
  exited never shut down.

The two fixes are pre-existing defects from the stdio transport (#481). They were fixed here at the
developer's request.

Feature: [opencode-cli-session.md](../../../../docs/ft/daemon/opencode-cli-session.md#sandbox-mode),
[muse-cli-session.md](../../../../docs/ft/daemon/muse-cli-session.md#sandbox-mode). Architecture:
[tddy-sandbox architecture](../../../tddy-sandbox/docs/architecture.md). PR
[#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
