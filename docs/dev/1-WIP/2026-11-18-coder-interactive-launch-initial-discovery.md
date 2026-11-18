# Initial discovery: An agent's terminal on a Coder workspace opens

**Date**: 2026-11-18

## How it was found

A temporary, uncommitted test drove `tddy_agent_launch::remote_agent_launch::launch_on_target` against
the real `mantasi/remote-dev` (coder v2.33.5). It used:

- the built-in `claude` spec, resolved on this Mac by `host_coding_agents`, as the daemon resolves it;
- the real `CoderShell`, with #939 applied;
- a qape-hq worktree cut there through `tddy_git::setup_remote_worktree`.

Each failure was worked around in the test, and the run repeated:

| Run | Failure | Workaround (test only) |
|---|---|---|
| 1 | `PTY spawn failed: spawn failed: Unable to spawn coder because: Unable to resolve the PATH` | `CoderShell::through("/opt/homebrew/bin/coder", …)` |
| 2 | `parsing flags (…) for "coder ssh": unknown shorthand flag: 't' in -t` | a `RemoteShell` wrapper whose `interactive` drops `-t` |
| 3 | `env: '/Users/mantasi/.local/share/claude/versions/2.1.311': No such file or directory`, exit 127 | the VM's `/home/coder/.local/bin/claude`, `HOME` and `PATH` (not this node) |
| 4 | `AgentNeverStarted { after: 150s }` while `claude` (pid 826584) ran on the VM, its pid recorded at 06:15:08 | the wrapper gives each one-shot at least 30 s |
| 5 | Claude's folder-trust dialog, then "13 new MCP servers found in this project" | answered from the test (Yes, then Esc). Not this node |
| 6 | the turn completes: `hostname: coder-mantasi-remote-dev`, the worktree's `pwd`, branch `claude-cli/fix-check-01a1ecee` | — |

## Code

- `packages/tddy-agent-launch/src/coder_target.rs`: `shell_for_coder_target` →
  `shell_for_coder_target_through("coder", …)`, a bare name.
- `packages/tddy-remote-shell/src/ssh.rs`: `SshShell::new` → `through("ssh", …)`, a bare name.
- `packages/tddy-agent-launch/src/remote_agent_launch.rs` `launch_on_target`: `local_env` is
  `interactive.env`, plus `SSH_AUTH_SOCK`, passed to `CliSessionManager::spawn_rendered_main`.
- `packages/tddy-cli-sessions/src/cli_session_manager/launch.rs` `spawn_rendered_main`:
  `tddy_pty::PtyEnvBase::Cleared`.
- `packages/tddy-pty/src/runtime.rs` `build_pty_command`: `env_clear()` for `Cleared`, then
  `spec.env`. portable-pty resolves `argv[0]` against that environment's `PATH`.
- `packages/tddy-remote-shell/src/coder.rs` `CoderShell::interactive`: `["ssh",
  "--disable-autostart", "-t", <workspace>, "--", <command>]`. The test
  `the_interactive_command_opens_a_terminal_on_the_workspace` pins the `-t`.
- `packages/tddy-agent-launch/src/remote_agent_launch/startup.rs`: `wait_for_agent_start` probes
  `cat .tddy/agent.pid` `within(5 s)`, and `exits_during_startup` probes `kill -0 <pid>`
  `within(5 s)`. A probe error counts as alive in the second and as not yet recorded in the first.
- `start_bound = startup_window × 10`, at least 5 s.

## Measured

- `coder ssh` round trip to `coder.ireland-devex.wixprod.net`: 6–8 s per one-shot.
- `coder ssh` without `-t`, run in a local PTY, gives the command a terminal. Claude's full-screen
  TUI rendered in it.
