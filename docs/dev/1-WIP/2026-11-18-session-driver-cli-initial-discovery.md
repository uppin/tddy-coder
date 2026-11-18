# Initial discovery: the session-driver CLI

**Date**: 2026-11-18

## What it replaces

The live checks of #939 and #941 against `mantasi/remote-dev` were temporary tests in
`tddy-agent-launch/tests/`, deleted after each run. They did by hand what this binary does:

- resolve `claude` with `host_coding_agents(<empty tools dir>).resolve("claude", None)`;
- build `HarnessLaunch { model, prompt, session_id, .. }` → `harness_template_params`;
- call `launch_on_target(&agent, &params, &RenderedSlots::default(), RemoteLaunch { shell, worktree,
  session_dir, pointer: None, startup_window }, HostPtySession { manager, .. }, None)`;
- poll `handle.capture.lock().buffered_bytes()`, strip ANSI, and match on text with whitespace
  removed. Claude places words with cursor moves, so "I trust this folder" arrives as
  `Itrustthisfolder`;
- answer dialogs with `handle.send_input(b"\x1b[B")` and then `b"\r"`, or `b"\x1b"`;
- clean up with `manager.stop_session` and a remote `pkill -f '[c]laude … --session-id <id>'`.

## Reusable pieces

| Need | Where |
|---|---|
| Resolve an agent as the daemon does | `tddy_agent_launch::host_coding_agents::host_coding_agents`, `HostCodingAgents::resolve` |
| Template params | `tddy_agent_launch::harness_from_spec::{harness_template_params, HarnessLaunch}` |
| Remote launch | `tddy_agent_launch::remote_agent_launch::{launch_on_target, RemoteLaunch, RemoteLaunched}` |
| Local launch | `tddy_agent_launch::spec_launch::{launch_host_pty, HostPtySession}` |
| Shells | `tddy_remote_shell::{CoderShell, CoderCredentials, CoderWorkspaceRef, SshShell}` |
| Terminals | `tddy_cli_sessions::cli_session_manager::CliSessionManager`; `PtyHandle { capture, pty_done, pid, send_input }` |
| Screen model | `tddy_task::TerminalCapture::text_grid`, `tddy_task::{crop, highlighted_rows, GridRegion}`: what `ReadSessionTerminal` answers |
| Key encoding | `tddy_tool_engine::terminal_keys::encode_keys`: what `SendSessionKeys` types |
| MCP over stdio | `rmcp` 1.x with `server`, `transport-io`, `schemars` and `macros`, as `tddy-tools/src/server.rs` uses it (`tool_router`, `tool_handler`) |
| Terminal tool semantics | `tddy-tool-engine/src/project_home_terminal_tools.rs` (descriptions, `region`, `styles`) |

## Observed on the real target

- One `coder ssh` call takes 6–8 s, so `wait_for_text` timeouts must be in tens of seconds.
- The Coder CLI prints a `👋 Your workspace is outdated!` banner on the terminal before the agent.
  It is harmless, but it is on the screen.
- Claude's first start in an untrusted worktree shows the folder-trust dialog. A qape-hq worktree
  then shows "13 new MCP servers found in this project". Both are answered once, and stay answered in
  the VM's `~/.claude.json`.
