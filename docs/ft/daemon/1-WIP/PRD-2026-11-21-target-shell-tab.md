# Target shell tab - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — `tabs.yaml` gains a `target` wrapper node.
- **Related Feature**: [Remote-managed worktree / session targets](../remote-managed-worktree.md) — a target session's tab runs on the target.
- **Related Feature**: [Session terminal tabs](../../web/session-terminal-tabs.md) — "On target" / "On this host" in the new-tab menu.

## Summary

A shell tab opened on a target session (ssh or Coder) runs **on the target**, in the session's worktree there, through the session's remote shell. It is recorded as `target` → `shell`, restored the same way on Resume, and dropped with its reason when the target cannot be reached. "On target" is the default; "On this host" stays available.

## Background

Today a target session's tab is a host shell in a path that exists only on the target, and Resume probably drops it. The only target launch, `launch_on_target`, is for the main agent: it writes the session's pid file, rewrites the session pointer, and stops the whole session if the agent does not start — none of which a tab may do.

## Proposed Changes

### What's Changing
- `StartTerminalSession` accepts a `target` wrapper around a shell; the daemon resolves the session's target shell (ssh alias or Coder workspace, with the owner's credentials) and runs `cd <worktree> && <login shell>` through its interactive client as a local tab terminal.
- A tab launch on a target writes no pid file, touches no session pointer, and on failure ends only the tab.
- Recorded as `[target, shell]`; Resume reopens it on the target; an unreachable target or a locked vault drops the tab with its reason.
- Web: for a target session the menu offers "On target" (default) and "On this host".

### What's Staying the Same
- The target main's launch and resume.
- Liveness and stop: a target tab is a local terminal like any other, so existing stop and delete apply.

## Impact Analysis

### Technical Impact
tddy-terminal-rpc (wrapper parsing), tddy-changeset (`target` kind), tddy-agent-launch (non-main target client launch, restore kind, target shell for the roster), tddy-session-lifecycle (roster start with the target shell), tddy-session-web (menu).

### User Impact
Extra shells of a remote session open where the code is.

## Implementation Plan

One node: `feature/session-liveness/target-shell-tab` (#session-liveness 6/8).

## Acceptance Criteria

- [ ] A shell tab opened "On target" runs on the target in the session's worktree.
- [ ] It writes no pid file and leaves the session pointer and the main untouched; its failure ends only the tab.
- [ ] It is recorded as `target` → `shell` and reopened on the target by Resume.
- [ ] An unreachable target or a locked vault drops the tab on Resume with its reason.
- [ ] The menu offers "On target" (default) / "On this host" only for target sessions.
