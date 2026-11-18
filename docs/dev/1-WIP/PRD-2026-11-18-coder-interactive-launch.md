# PRD: An agent's terminal on a Coder workspace opens, and its start is judged at the deployment's pace

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 2/3 — branch `feature/coder-start/interactive-launch`, base `fix/address-01a1ecee-findings` (#939)

## Problem

With #939, a Claude start on a Coder workspace writes its settings file and reaches the agent step.
A live run of `launch_on_target` against `mantasi/remote-dev` (coder v2.33.5, 2026-11-18) then
failed three times before any agent could start, each time for a reason the stand-in suites cannot
see:

1. **The terminal's client is not found.** The daemon reaches a Coder workspace through the bare
   name `coder` (`shell_for_coder_target`), and an ssh target through the bare name `ssh`
   (`SshShell::new`). The session's local PTY is spawned with a cleared environment that carries
   only the transport's credentials and `SSH_AUTH_SOCK`, so the spawn fails:
   `Unable to spawn coder because: Unable to resolve the PATH`. The stand-ins are always handed by
   absolute path, so no test spawns a bare name.
2. **The CLI rejects the terminal flag.** `CoderShell::interactive` runs `coder ssh -t …`.
   coder v2.33.5 has no `-t` flag (`unknown shorthand flag: 't' in -t`), and `coder ssh` gives
   the command a terminal of its own when its stdin is one. The stand-in accepts any flag.
3. **The start is judged by probes shorter than one call.** Once the terminal runs, the launch waits
   for the agent's pid file and then watches the agent's liveness, with each probe a one-shot bounded
   by 5 s. One `coder ssh` round trip to that deployment takes 6–8 s, so every probe times out. The
   launch failed `AgentNeverStarted` after 150 s while Claude was running on the workspace. The
   failed start stopped its local terminal and left that agent orphaned there.

With these three worked around, Claude started on the workspace and stopped at its folder-trust
dialog. Answered by hand, it completed its turn.

## Requirements

1. **A transport client named by a bare program is found** on the daemon's `PATH`, for both Coder and
   ssh targets. The agent's own environment is unchanged: it still travels inside the remote command
   under `env -i`.
2. **A Coder terminal opens on coder v2.33.5:** `CoderShell::interactive` is
   `coder ssh --disable-autostart <workspace> -- <command>`, with no `-t`.
3. **The start probes are bounded by the start's own bounds, not a fixed 5 s.**
   - Waiting for the pid file gives each probe the time left until the start bound.
   - Watching liveness gives each probe the time left in the startup window.
   - On a transport slower than 5 s per call, an agent that records its pid is judged started, and an
     agent that exits during the window still fails the launch.

## Acceptance criteria

- [ ] A Coder target reached through a bare `coder` found only on the daemon's `PATH` launches its
      agent (`tddy-agent-launch`, a test binary of its own that sets `PATH`)
- [ ] A Coder workspace whose CLI refuses `-t`, as v2.33.5 does, launches its agent
- [ ] `CoderShell::interactive` carries no `-t`
- [ ] On a stand-in that takes longer than 5 s per one-shot, an agent that stays up is launched,
      and one that exits during the window fails the launch with its last output
- [ ] Scoped gates: `./test -p tddy-remote-shell -p tddy-agent-launch`, and `cargo clippy` on both
      with `-D warnings`

## Out of scope

- The agent's binary path, `HOME` and `PATH` on a target (backlog
  `2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path`).
- Claude's folder-trust and project MCP-server dialogs.
- The Cursor chat mint.
- A remote process outliving a deadline.
- `exec_streamed` through the pty.
