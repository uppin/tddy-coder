# PRD: Session Agents — Peer Agent Sessions for Multi-Agent Collaboration

**Status:** 🚧 In Progress (implementation)
**Created:** 2026-07-27
**Branch:** `feat-add-session-agent`

## Summary

Introduce **session agents** — peer agent sessions sharing the current session's workspace, each with its own coding backend (e.g. Cursor alongside Claude). The operator spawns a peer from a new entry point inside the per-session `SessionMainPane` (the session detail view) and switches between peers in a new "Session agents" section. Agents co-exist and the operator switches between them; there is **no agent-to-agent messaging** in v1.

This reuses the existing `stack_parent` / `orchestratorSessionId` child-session infrastructure — **no proto or daemon changes**, no new external dependencies. It is a tddy-web-only feature.

## Background and Motivation

Today a tddy-coder session runs **one** coding backend (claude / cursor / codex-acp / the `--acp` workflow agent). The web already supports *child* sessions via the `orchestratorSessionId` field on `SessionEntry` (field 21): `PrStackScreen` uses this to spawn PR-stack child sessions, pre-filling `CreateSessionInitialValues` with `stackParent` / `projectId` / `daemonInstanceId` (see [PrStackScreen.tsx:137-157](../../../packages/tddy-web/src/components/sessions/prstack/PrStackScreen.tsx)). The `StartSession` RPC already accepts `stack_parent` (field 15), which the spawned coder receives as `--stack-parent <id>`.

However, the only entry points to spawn a child session today are workflow-specific (the PR-stack orchestrator's "Start session" CTA) or external (tddy-tools/MCP). There is **no generic, per-session UI** to spawn a peer agent on the session the operator is already looking at, and **no view** that lists the peers attached to the current session.

The "session agents" feature closes that gap: a generic "Add agent" entry point in `SessionMainPane` that spawns a peer child session sharing the current session's workspace, plus a "Session agents" section that lists the peers with status and quick-switch. The operator gets multi-agent collaboration (e.g. a Cursor session reviewing a Claude session's work in the same workspace) by reusing infrastructure that already exists.

## Affected Features

This is a new feature that plugs into existing session-detail surfaces. It modifies:

- [session-drawer.md](../session-drawer.md) — the session detail pane (`SessionMainPane`) gains a new "Add agent" entry point and a "Session agents" section; the drawer's session list is unchanged.

It does **not** modify:

- `CreateSessionPane` — reused as-is; the peer spawn pre-fills its existing `initialValues` (`stackParent`, `projectId`, `daemonInstanceId`, `sessionType`, `baseBranchLabel`, `branchIntent`).
- `AgentActivityOverlay` / `StreamAcpReplay` — the per-agent transcript view is unchanged; each peer is a normal session with its own transcript.
- The primary agent's workflow or coding backend selection.
- Any proto or daemon code — `StartSession.stack_parent` and `SessionEntry.orchestratorSessionId` are reused verbatim.

## Proposed Changes

### What's Changing

1. **New "Add agent" button in `SessionMainPane` header** — a peer of the existing `Code` / `Inspector` / activity-overlay toggles. Clicking opens `CreateSessionPane` pre-filled to spawn a peer child session that runs on the **same worktree** as the current session: `stackParent = selectedSession.sessionId`, same `projectId` / `daemonInstanceId`, `repoPath = selectedSession.repoPath` (sets `StartSession.repo_path` → the daemon uses that path as the worktree, no new worktree, no branch checkout), operator picks a separate `sessionType` / `agent` / `model`. Branch selection is **hidden** in this peer mode (irrelevant when `repo_path` is set).
2. **New "Session agents" section in `SessionMainPane`** — lists the current session's peers (sessions with `orchestratorSessionId === selectedSession.sessionId`), each showing `sessionId` / `agent` / `model` / `status`, with a "switch" action that focuses the peer's runtime. Empty state when there are no peers.
3. **Peer spawn wiring in `SessionsDrawerScreen`** — reuses the existing `onChildSessionStarted` optimistic-overlay path so a spawned peer appears in the list immediately.

### What's Staying the Same

- The primary agent's workflow, coding backend, model selection, and ACP session — untouched.
- `CreateSessionPane`'s form and field set — reused as-is (the peer flow only adds pre-fill).
- `AgentActivityOverlay`'s read-only transcript — untouched; each peer has its own.
- Session lifecycle (start / resume / delete / terminate) — untouched; a peer is a normal child session.
- The `SessionEntry` proto and `StartSession` RPC — no field changes.
- The `stack_parent` / `orchestratorSessionId` semantics — reused verbatim.

## Impact Analysis

### Technical Impact

- **tddy-web** (only affected package):
  - New `SessionAgentsSection.tsx` component (peers list + status + switch).
  - New `sessionPeers.ts` util (derive peers from the `sessions` list).
  - `SessionMainPane.tsx` gains the "Add agent" button + mounts `SessionAgentsSection`.
  - `SessionsDrawerScreen.tsx` wires peer spawn via the existing `onChildSessionStarted` path.
- **tddy-coder / tddy-daemon**: no changes.
- **proto**: no changes.

### User Impact

- Operators can spawn a second agent (e.g. Cursor) alongside the current session's agent (e.g. Claude), sharing the workspace, from the session detail view.
- The session detail view gains one new header button and one new section; existing toggles are unchanged.
- No change to existing sessions' behavior — the section shows an empty state when there are no peers.

## Implementation Plan Overview

Detailed in the changeset (`docs/dev/1-WIP/2026-07-27-session-agent.md`). High-level phases:

1. **`sessionPeers.ts`** — pure util, TDD: peers = child sessions of the current session.
2. **`SessionAgentsSection.tsx`** — TDD: list peers, status, switch action, empty state.
3. **`SessionMainPane`** — TDD: "Add agent" button opens `CreateSessionPane` with peer pre-fill; mount `SessionAgentsSection`; switch focuses the peer's runtime via the existing `focusedRuntimeId` mechanism.
4. **`SessionsDrawerScreen`** — TDD: peer spawn reuses `onChildSessionStarted` optimistic overlay.

## Acceptance Criteria

1. An operator viewing a session in `SessionMainPane` sees an "Add agent" button; clicking opens `CreateSessionPane` pre-filled to spawn a peer that runs on the **same worktree** as the current session (`stackParent` = current session, `repoPath` = current session's `repoPath`, same project/daemon, separate agent backend pickable). Branch selection is hidden in this peer mode.
2. A new "Session agents" section lists the session's peers (children via `orchestratorSessionId`), each with `agent` / `model` / `status`.
3. The operator can switch to a peer from the section; the peer's runtime becomes focused.
4. Spawning a peer uses the existing `stack_parent` + `repo_path` path — no proto/daemon changes, no new external deps.
5. Existing sessions with no peers behave identically (no new UI noise when the section is empty).
6. Strict TDD: every behavior is covered by a failing acceptance test written before implementation, per the repo's `fluent-tests` style.

## Design Decisions (resolved during planning)

- **Backend involvement** → reuse existing `stack_parent` / `orchestratorSessionId`; no proto/daemon changes.
- **Companion model** → separate pick: the operator chooses the peer's `sessionType` / `agent` / `model` in `CreateSessionPane`.
- **Collaboration model** → co-exist + switch; no agent-to-agent messaging in v1.
- **Persistence** → a peer is a normal session; it survives restart via the existing resume path.
- **Multi-peer** → v1 supports multiple peers (all children listed); no artificial cap.
- **Same worktree (no branch selection)** → a peer runs on the **same worktree** as the current session. The peer spawn sets `StartSession.repo_path` (field 24) to the current session's `repoPath`; per the daemon contract, a non-empty `repo_path` makes the session's worktree BE that path — no git worktree is created and no branch is checked out. Branch selection (`branch_worktree_intent` / `new_branch_name` / `selected_branch_to_work_on` / `create_remote_branch`) is therefore **irrelevant and hidden** in the peer spawn flow. The operator only picks the peer's `sessionType` / `agent` / `model` / `initialPrompt`. This keeps two agents editing the same checkout (genuine co-existence) and avoids surprising the operator with controls that have no effect.

## References

- [agent-activity-pane.md](../agent-activity-pane.md) — `StreamAcpReplay`, `AgentActivityRegistry`; each peer has its own transcript via the same RPC.
- [session-drawer.md](../session-drawer.md) — `SessionMainPane` structure the new entry point and section plug into.
- [session-participant-rpc.md](../../coder/session-participant-rpc.md) — `stack_parent` / `orchestratorSessionId` semantics reused verbatim.
- [managed-codebase-subagents.md](../../coder/managed-codebase-subagents.md) — existing specialized-subagents concept (distinct: subagents are tools the primary agent calls; a session agent is a peer session the operator switches to).
