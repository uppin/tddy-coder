# 2026-11-18 — A target launch finds its client and judges its start at the deployment's pace

**Type:** Fix

- `launch_on_target`'s local PTY also carries the daemon's own `PATH`, so a bare `coder` or `ssh` is
  found; the agent's environment still travels inside the remote command.
- `wait_for_agent_start` and `exits_during_startup` bound each probe by the time left in the start
  bound or startup window, not a fixed 5 s, so an agent on a deployment slower than 5 s per call is
  judged started and one that exits still fails the launch.

Pinned by `coder_agent_launch_on_path_acceptance` (a binary of its own, since it sets `PATH`) and new
cases in `coder_agent_launch_acceptance`. See [module-layout.md](../module-layout.md). Full record:
[`docs/dev/changesets/2026-11-18-coder-interactive-launch.md`](../../../../docs/dev/changesets/2026-11-18-coder-interactive-launch.md).
