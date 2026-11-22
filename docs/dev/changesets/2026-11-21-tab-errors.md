# 2026-11-21 — Target tab errors say what went wrong

**Type:** Feature · **Stack:** `#tab-hardening` 1/4 (PR #989) · **Packages:** `tddy-agent-launch`,
`tddy-session-lifecycle`, `tddy-testing-commons` (test kit) ·
Feature: [session-liveness.md](../../ft/daemon/session-liveness.md#a-target-sessions-tabs)

## Problem

A target agent tab mapped every launch failure to `FAILED_PRECONDITION` (an unreachable target included, which a
shell tab already reported `UNAVAILABLE`), a local terminal that could not spawn was `FAILED_PRECONDITION` for both
kinds, and an agent the target lacked showed up only as exit `127` inside a tab that had already opened. The cursor
tab test pinned only the first three argv entries.

## What changed

- **`remote_agent_launch/tab_launch_status.rs`** — `tab_launch_status(&RemoteLaunchError) -> Status`, the one
  mapping shell and agent tabs share (`target_tabs.rs`'s `reach_failure` is gone): `Shell` and the pre-trust
  `Trust(Shell)` are `UNAVAILABLE`, `Spawn` is `INTERNAL`, `AgentNotOnTarget` and every other variant
  `FAILED_PRECONDITION`.
- **`remote_agent_launch/agent_probe.rs`** — `probe_agent_on_target(shell, worktree, command)`: one call through the
  port, `cd '<wt>' && exec "${SHELL:-/bin/sh}" -lc 'command -v <cmd>'`, quoted with `tddy_core::ssh_exec::shell_single_quote`;
  a non-zero exit is the new `RemoteLaunchError::AgentNotOnTarget { command }`. `launch_agent_tab_on_target` calls
  it after `reach_the_worktree`, before any terminal is spawned (new and restored tabs; a restore drops the tab
  with the error's words).
- **Test kit** — `StandInSshTarget::behind_ssh_with_only_the_system_path()` (testing-commons) and the lifecycle
  fake's `-lc` `PATH` of its agents directory plus `/usr/bin:/bin`: the stand-in targets no longer inherit this
  host's `PATH`, which made "the agent is missing on the target" untestable on a host with the agent installed.
  The lifecycle fake gains `target_shells_whose_client_this_host_lacks()`, the PTY spawn-failure seam.
- The target cursor tab's full argv (`agent --resume <chat> --trust`) is pinned.

## Decisions & Trade-offs

- **An unreachable target is `UNAVAILABLE` at every step** (developer decision 2026-11-21), including
  `Trust(Shell)` during a start's clone pre-trust; `Trust(Malformed)` stays `FAILED_PRECONDITION`.
- **Every agent tab is probed, new and restored** (developer decision): one extra round trip per tab, about 6–8 s
  on a Coder workspace, accepted. The probe is pinned on a stand-in target only
  ([todo](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)).
- **Spawn failure is `INTERNAL`**: a local resource problem, not a precondition the caller can meet.
- **The probe reads only the exit status**; a worktree the target lacks is caught before it, by
  `reach_the_worktree`. The helper is reused by the target session start (`#tab-hardening` 4/4).
- **The probe quotes with the shared `shell_single_quote`** (`'"'"'` for an embedded quote), not a private copy.

## Tests

`tddy-agent-launch`: `target_agent_tab_acceptance` (`an_agent_tab_whose_agent_is_missing_on_the_target_fails_before_its_terminal_starts`,
`an_agent_tab_on_an_unreachable_target_fails_unavailable`), unit tests of `tab_launch_status` and
`probe_agent_on_target`. `tddy-session-lifecycle`: `target_agent_tab_acceptance` (unreachable is `UNAVAILABLE`,
missing agent names the command, an unspawnable terminal is `INTERNAL`) and
`resume_tab_roster_acceptance::a_target_agent_tab_whose_agent_is_missing_is_dropped_naming_it_on_resume`.
