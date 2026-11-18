# Changeset: An agent's terminal on a Coder workspace opens, and its start is judged at the deployment's pace

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Fix
**Complexity**: 2. Two crates and three small, separate defects: the local PTY's environment, one
argv flag, and two probe deadlines. Each is pinned by a stand-in that behaves as the real
deployment did.
**Stack**: `#coder-start` 2/3 — branch `feature/coder-start/interactive-launch`, base
`fix/address-01a1ecee-findings` (#939)

## Initial Discovery

[`2026-11-18-coder-interactive-launch-initial-discovery.md`](2026-11-18-coder-interactive-launch-initial-discovery.md)

## Prerequisites

### ⚠ DURING — target-session test fixtures are duplicated — [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)

This node adds one more stand-in behaviour (a CLI that refuses `-t`, a slow deployment) to
`coder_agent_launch_acceptance.rs`'s own `AWorkspace`. It does not consolidate the fixtures; the
entry stays open.

### ℹ Not this node — the target launch runs the daemon host's agent path — [`2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md`](../todo/2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md)

This was confirmed live after fix 2: `env: '/Users/…/claude/versions/2.1.311': No such file or
directory`, exit 127. It needs a design decision and stays open.

## Affected Packages

- `tddy-remote-shell`: `CoderShell::interactive`.
- `tddy-agent-launch`: `remote_agent_launch.rs` (the local PTY's environment) and
  `remote_agent_launch/startup.rs` (the probe deadlines).

## Scope

- [ ] Fix 1: the transport client is found on the daemon's `PATH`
- [ ] Fix 2: no `-t` in `CoderShell::interactive`
- [ ] Fix 3: start probes bounded by the start's own bounds
- [ ] Each fix pinned by a test that fails before it
- [ ] Live retry of a Claude start on `mantasi/remote-dev`, recording where it stops
- [ ] Scoped gates for `tddy-remote-shell` and `tddy-agent-launch`

## Responsibility

This PR gets an agent's terminal on a Coder (or ssh) target to open, and the agent's start judged
correctly on a slow deployment.

## Boundaries

- It does not choose the agent's binary, `HOME` or `PATH` on the target.
- It does not answer Claude's trust or MCP dialogs.
- It does not mint a Cursor chat on the target.
- It does not change `exec_streamed`, or what a timed-out one-shot leaves on the workspace.
- It does not build the session-driver CLI (branch `feature/coder-start/session-driver-cli`, node 3).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #939 (`fix/address-01a1ecee-findings`) | `CoderShell::exec` one-shots that cross the workspace's pty intact (base64 stdin, `stty -opost`, `/dev/null` stdin) | the launch's `mkdir`, slot-file writes and probes run through them | change the one-shot wrapper |

## Draft PR contract

The draft opens with the planning documents. The failing tests (fixes 1–3) and their fixes follow
in this same PR. Node 3 consumes nothing new from this PR's API: it uses `launch_on_target` and
`CoderShell` as they already are.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes. Every test drives its own stand-in `coder`. None needs node 3.
**Concurrent with:** node 3's planning
**Blocks:** node 3's live verification. Its Coder runs need fixes 1–3 to reach the agent.

## Technical Changes

### State A

- `launch_on_target` spawns the local PTY with `PtyEnvBase::Cleared`, plus the transport's
  credentials and `SSH_AUTH_SOCK`. A bare `argv[0]` (`coder`, `ssh`) cannot be resolved.
- `CoderShell::interactive` is `coder ssh --disable-autostart -t <workspace> -- <command>`.
- `wait_for_agent_start` and `exits_during_startup` bound each one-shot probe by 5 s.

### State B

- The local PTY also carries the daemon's own `PATH`: the client's lookup path. It is not the
  agent's, whose environment still travels inside the remote command.
- `CoderShell::interactive` is `coder ssh --disable-autostart <workspace> -- <command>`.
- Each pid-file probe is bounded by the time left to the start bound, and each liveness probe by
  the time left in the startup window.

## Acceptance Tests

See the PRD's acceptance criteria. New tests:

- `tddy-agent-launch/tests/coder_agent_launch_on_path_acceptance.rs`, a binary of its own, because
  it sets `PATH` for its process.
- In `coder_agent_launch_acceptance.rs`:
  - a workspace whose CLI refuses `-t`;
  - a slow deployment, with an agent that stays up and one that exits.
- `tddy-remote-shell/tests/coder_shell_acceptance.rs`: the interactive argv.
