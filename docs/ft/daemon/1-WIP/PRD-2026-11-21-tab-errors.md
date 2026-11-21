# Target tab errors say what went wrong - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — the errors a target tab start reports.
- **Related Feature**: [Remote-managed worktree / session targets](../remote-managed-worktree.md).

## Summary

A target tab that cannot start says why, with a code a client can act on: an unreachable target is `UNAVAILABLE` for agent tabs too (it already is for shell tabs), a local terminal that cannot spawn is `INTERNAL`, and an agent tab first checks that its agent exists on the target, failing with a clear "not installed on the target" error instead of a tab that exits 127. Cursor's `--trust` on a target tab is pinned.

## Background

After `#session-liveness`, a target agent tab maps every failure to `FAILED_PRECONDITION`, a local spawn failure is `FAILED_PRECONDITION` for both kinds, and a missing agent shows up only as exit 127 inside the tab.

## Proposed Changes

### What's Changing
- Agent and shell tabs share one error mapping: transport failure → `UNAVAILABLE`, vault locked or account missing → `FAILED_PRECONDITION`, local spawn failure → `INTERNAL`, agent missing → `FAILED_PRECONDITION` naming the command.
- Before launching an agent tab on a target, the daemon runs `command -v <agent>` through the target's login shell (`probe_agent_on_target`, reused by the session start in #tab-hardening 4/4). New tabs and restored tabs both probe.
- A target cursor tab's argv is pinned in full (including `--trust`).

### What's Staying the Same
- Shell tabs (no probe), host tabs, the target main.

## Acceptance Criteria

- [ ] An agent tab on an unreachable target fails `UNAVAILABLE` with the transport's message.
- [ ] A tab whose local terminal cannot spawn fails `INTERNAL`.
- [ ] An agent tab whose agent is missing on the target fails before any terminal is started, naming the command; on Resume it is dropped with that reason.
- [ ] A target cursor tab runs `agent --resume <chat> --trust`.
