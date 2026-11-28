# Coder-target sessions get no `SSH_AUTH_SOCK` — `coder ssh` runs without agent forwarding

**Found:** 2026-11-28, investigating session `01a22204-3406-77b2-8845-60a2e59b0968`
(project `f3dfec76-…`, `custom-editor-playground`, **coder target `mantasi/remote-big`**, owner
`uppin`, hosted by the local Tddy Desktop daemon on macOS). Investigation only — nothing fixed.

## Symptom

Inside the session `SSH_AUTH_SOCK` is empty; `git` over SSH fails `Permission denied (publickey)`
until the agent prefixes `SSH_AUTH_SOCK=$HOME/.ssh/agent.sock` (a workspace-local agent that happens
to exist on `remote-big`). Restarting the session did not change that.

## Root cause

The agent does not run on the Mac; it runs on the Coder workspace, launched through
`coder ssh --disable-autostart <owner/ws> -- <command>`:

- `packages/tddy-remote-shell/src/coder.rs:475-490` — `CoderShell::interactive` builds that argv
  with **no `--forward-agent` (`-A`)**. The OpenSSH path (`exec_streamed` / `ssh_command`) also
  sets no `-A`.
- `packages/tddy-agent-launch/src/remote_agent_launch.rs:525-544` — the transport client's local
  env *does* receive the daemon's `SSH_AUTH_SOCK` (the comment says so: "it carries this daemon's
  ssh environment (its agent socket)"), so the local half is ready. The daemon has one:
  `SSH_AUTH_SOCK=/var/run/com.apple.launchd.…/Listeners` (launchd's, observed on the running
  Desktop process tree). Without `--forward-agent` the coder CLI never exposes it remotely, so the
  remote shell, the tmux server it starts and the agent all lack the variable.

Not the cause (ruled out): the supervisor / spawn allow-lists (`tddy-spawn/src/clone_env.rs:117`
sets `SSH_AUTH_SOCK` only for daemon-run *clones*, not agent tabs); the Desktop/launchd env (the
daemon has a socket).

## Why a restart does not help

Daemon log, 14:07:45–14:09:47: `resuming claude-cli session … `, repeated
`re-attach … skipped, its tmux session is gone`, then `resumed … (pid 89825)`. The resume
re-runs the **same** launch — `target_agent_tmux_tab_command`
(`packages/tddy-agent-launch/src/remote_agent_launch/target_tmux_tab.rs:127`,
`tmux new-session -d -s tddy-<id> …; tmux attach-session`) over the same forwarding-less
`coder ssh`. Nothing was captured at creation; the env is simply never provided.

Secondary trap for the fix: even once forwarded, the agent pane inherits the **remote tmux
server's** environment. A server already running on the workspace (other `tddy-*` sessions live
there) keeps whatever `SSH_AUTH_SOCK` it started with, and a forwarded socket path changes per
connection. `update-environment` only refreshes the *session* env on attach, for new panes.

## Proposed fix (needs operator decision)

1. Add `--forward-agent` to `CoderShell::interactive` (and `-A` / `ForwardAgent=yes` on the
   OpenSSH path) — opt-in per target or per project, since forwarding grants the workspace use of
   the laptop's keys while connected.
2. Make the agent pane robust to tmux: pass the forwarded socket explicitly into the agent's
   command (`tmux new-session -e SSH_AUTH_SOCK=…`, or a stable symlink such as
   `~/.ssh/tddy-agent.sock` refreshed by each launch/attach and exported in the inner command).
3. Acceptance test in `tddy-agent-launch` target-tab suites: argv carries `--forward-agent` when
   enabled; agent command exports the socket.

Operator workaround today: a workspace-local agent (`~/.ssh/agent.sock` on `remote-big`) exported
from the workspace's login shell profile, so `-lc` picks it up.
