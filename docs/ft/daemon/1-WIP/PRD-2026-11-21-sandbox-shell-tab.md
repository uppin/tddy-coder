# Sandbox shell tab - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — a sandboxed session's tab can run in a jail of its own; `tabs.yaml` gains a `sandbox` wrapper node.
- **Related Feature**: [Session terminal tabs](../../web/session-terminal-tabs.md) — the new-tab menu offers "In sandbox" / "On host" for a sandboxed session.
- **Related Feature**: [Claude CLI session — sandboxed](../claude-cli-session.md) and [Terminal sessions](../terminal-sessions.md) — a session may now own several jails: its main's and one per jailed tab.

## Summary

A shell tab opened on a sandboxed session runs in its **own jail** — a second `tddy-sandbox-runner` with the session's worktree and sandbox profile — and is recorded as `sandbox` → `shell` in `tabs.yaml`, so Resume restores it into a new jail. The user picks "In sandbox" (default) or "On host" per tab; both kinds can coexist in one session.

## Background

Today a sandboxed session's secondary tab is an unjailed host shell, and it cannot even be streamed while the main jail is registered (the terminal store answers only `main` for a sandboxed session). The jail's whole point — confinement of what runs against the worktree — stops at the first extra tab.

## Proposed Changes

### What's Changing
- `StartTerminalSession` accepts a `sandbox` wrapper around a shell; the daemon starts a per-tab jail (own runner, own IPC socket, ready marker, exec plan and egress log directory) with the session's worktree mounted and the session's sandbox grants.
- The tab's output streams like any tab; it is listed, closable, and recorded as `[sandbox, shell]`.
- Resume restores a `[sandbox, shell]` tab into a new jail; if the jail cannot start, the tab is dropped with its reason and the resume carries on.
- "On host" tabs of a sandboxed session are now reachable (the store routes every non-main id to its owner).
- Closing the tab, deleting or resuming the session, and daemon shutdown stop every tab jail. Daemon shutdown also stops the **main** jails (today it leaves them running).
- Web: for a sandboxed session the new-tab menu offers "In sandbox" (default) and "On host"; `SessionEntry` says whether a session is sandboxed.

### What's Staying the Same
- The main jail, its launch and its resume.
- Jailed tabs, like the main jail, cannot be resized and need no control lease (developer decision; separate work).
- Shell tabs never make a session live (n1's rule).

## Impact Analysis

### Technical Impact
tddy-terminal-rpc (wrapper parsing), tddy-changeset (`sandbox` kind), tddy-daemon-sandbox (per-tab jail registry, per-tab jail paths), tddy-agent-launch (tab-jail launch, restore kind), tddy-session-lifecycle (roster start/end, store routing, shutdown), tddy-service proto (`SessionEntry.sandbox`), tddy-session-web (menu).

### User Impact
A sandboxed session's extra shells are confined by default and survive Resume.

## Implementation Plan

One node: `feature/session-liveness/sandbox-shell-tab` (#session-liveness 5/8).

## Acceptance Criteria

- [ ] A shell tab opened "In sandbox" on a sandboxed session runs in a jail of its own, with the session worktree as its working directory.
- [ ] It is recorded as `sandbox` → `shell`, and Resume restores it into a new jail.
- [ ] A jail that cannot start on Resume drops only that tab, with its reason.
- [ ] An "On host" tab of a sandboxed session streams and is recorded as plain `shell`.
- [ ] Closing the tab, deleting the session, and daemon shutdown leave no jail running (main jails included at shutdown).
- [ ] The menu offers "In sandbox" (default) / "On host" only for sandboxed sessions.
