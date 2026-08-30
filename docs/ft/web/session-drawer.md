# Session Drawer Screen

**Route:** `#/sessions` — also the **default** route (`#/` resolves here)
**Component:** `SessionsDrawerScreen` (`packages/tddy-web/src/components/sessions/`)

## Overview

A focused session management screen with a left-side drawer listing all sessions and a
main content area on the right. A Session Inspector shows session details and controls;
for connected sessions it is a right-edge overlay drawer (hidden by default), and for
disconnected sessions it **docks as the main pane** (open by default).

This is the default daemon-mode screen: `#/` resolves to it, and it renders inside the
shared [`AppShell`](app-shell.md) (`variant="fullbleed"`), so it carries the unified
top-left hamburger menu. The legacy `ConnectionScreen` (`#/`) it replaced has been removed.
A `#/sessions/<unknown-id>` deep link that matches no known session shows a "session not
found" state with a Home link.

## Layout

```
┌────────────────────┬──────────────────────────────┐
│  SessionDrawer     │  SessionMainPane              │
│  ─────────────     │                               │
│  ● my-feature      │  [when connected]             │
│  ○ old-branch      │  Terminal container           │
│  ◐ waiting         │           ┌──────────────────┤
│                    │           │ SessionInspector  │
│                    │           │ (overlay, ~360px) │
│                    │  [when disconnected]          │
│                    │  "Select a session…"          │
│                    │  (inspector open by default)  │
└────────────────────┴───────────┴──────────────────┘
```

## Drawer Items

Each `SessionDrawerItem` shows:
- A **status dot**: green (`connected`), grey (`disconnected`), yellow (`needs-input`)
- A **derived label** via `sessionDrawerLabel()`:
  1. `basename(repoPath)` — if non-empty
  2. `workflowGoal` — fallback
  3. `sessionId.slice(0, 8)` — last resort
- A **tooltip** on focus/hover revealing the full session id

Sessions are ordered newest-first by `createdAt` (`sortSessionsByCreation()`). Within that order
the list is split into an **Active** and a **Remaining** partition (see
[Active / Remaining Partition Separator](#active--remaining-partition-separator)); there is no
per-session active-first re-sort (contrast with `sortSessionsForDisplay` used by `ConnectionScreen`).

## Connection Status Token

`connectionStatusForSession(entry)` maps proto fields to one of:
- `"connected"` — `isActive: true` and `pendingElicitation: false`
- `"needs-input"` — `pendingElicitation: true` (takes precedence over `isActive`)
- `"disconnected"` — `isActive: false` and `pendingElicitation: false`

## Active / Remaining Partition Separator

The open drawer splits its session list into two partitions with a collapsible separator header
between them, so live sessions stay at the top and finished ones tuck away:

- **Active partition** — sessions whose status dot is **green or yellow**, i.e.
  `connectionStatusForSession(entry) !== "disconnected"` (equivalently `isActive || pendingElicitation`).
  A session blocked waiting for human input (`"needs-input"`, yellow) counts as active so it is not
  hidden. Header label: **`Active (N)`**. Expanded by default.
- **Remaining partition** — sessions whose dot is **grey** (`"disconnected"`). Header label:
  **`Remaining (M)`**. Collapsed by default.

`N` and `M` are the number of sessions in each partition (parents + children + flat rows).

**Stack-group nesting is preserved within each partition.** Partitioning happens first, keying on
each session's *own* status; each partition is then stack-grouped independently by
`groupSessionsByStack`. A PR-stack whose orchestrator and children share the same activity state
stays nested inside one partition. A **mixed-activity** stack (e.g. a live child under a finished
orchestrator) splits: each session lands in the partition matching its own dot, and a child whose
orchestrator is not present in the same partition renders as a flat row there.

**The separator only appears when both partitions are non-empty.** When every session is active
(or every session is disconnected) the drawer renders a single plain list with no separator header —
identical to the pre-partition layout.

**Collapse behaviour** mirrors the PR-stack group: clicking a partition header toggles the
visibility of that partition's rows. The two partitions collapse independently.

**Bulk delete is unaffected.** Selection mode (the bottom minibar) force-expands both partitions so
every row's checkbox is reachable; select-all and delete operate across both partitions in the
existing selection (insertion) order.

Utility: `partitionSessionsByActivity(sessions)` in `utils/sessionStackGroups.ts` returns
`{ active, remaining, activeCount, remainingCount }` where `active`/`remaining` are each a
`SessionStackGroupResult` (`groups` + `flat`). Components: `SessionDrawerSeparator`
(`components/sessions/SessionDrawerSeparator.tsx`) renders one collapsible header + its partition body.

## Cross-Host Active Sessions

The drawer shows a session that currently has a **live LiveKit participant** regardless of the
selected host. The list is the **union** of two sources:

- **The selected host's sessions** — from its `ListSessions` (active *and* inactive/history rows).
- **Live cross-host sessions** — every session with a coder participant in the common room, across
  all hosts.

There is **no `ListSessions` fan-out** and **no backend liveness signal**. Liveness *is* LiveKit
participant presence: a session's coder process joins the shared common room as
`daemon-<instanceId>-<sessionId>` (or `daemon-<sessionId>` on a single daemon), and the LiveKit SDK
keeps that participant alive while the process lives (connection-level keep-alive; a dead process ⇒
`ParticipantDisconnected`). The web is already in that room, so it observes those participants
directly via `useRoomParticipants` — real-time and independent of each owning daemon's version.
`parseSessionParticipantIdentity` (`utils/crossHostSessions.ts`) reads the owning instance id and the
(trailing-UUID) session id straight from the identity; a live session the selected host didn't return
is added as a minimal synthesized row owned by its host (label falls back to the short session id).
Cross-host visibility only applies when a common room exists — a single-daemon deployment has one
host and no cross-host case.

### `SessionManager`

The merged list, its refresh, and its change events live in one place: `SessionManager`
(`components/sessions/sessionManager.ts`), a plain observable store (no React/RPC dependency of its
own — those are injected). It unions the selected host's fetched sessions with the live participants,
de-dupes by `sessionId` (a metadata-carrying fetched row wins over a synthesized one), and sorts
newest-first. `useSessionManager` binds the RPC client, common-room participants, and selected host
into it and exposes the reactive list via `useSyncExternalStore`.

Refresh is decoupled from React through a **window-bound bridge** (`lib/sessionsRefreshBridge.ts`,
mirroring `terminalZoomBridge`): any screen calls `requestSessionsRefresh()` and the manager re-pulls
the selected host's sessions. (Active cross-host rows need no refresh — presence updates them live.)

**Owning-host badge** — a drawer item whose owning host differs from the selected host renders a
small muted host-label badge (`DaemonHost.label`, with the `" (this daemon)"` suffix stripped).

**Owning-daemon routing** — selecting a cross-host row does **not** change the selected host.
Attach/resume/delete/terminate RPCs for the selected session route to that session's **owning**
daemon (`useDaemonClientFor(ConnectionService, owningHost)`), while the **create** flow targets the
selected host. Because selection never calls `selectDaemon`, the screen does not remount.

## Session Attachment

`useSessionAttachment` hook manages the single-session attach lifecycle:
- `connectSession` → calls `ConnectSession` RPC → `connected-livekit` or `connected-grpc`
- `resumeSession` → calls `ResumeSession` RPC → same state transitions
- Clicking a connected session in the drawer auto-calls `connectSession`
- Clicking a disconnected session opens the inspector by default without auto-connecting

## Fast Session Change

The drawer keeps one self-contained **runtime** per attached LiveKit session, so switching
between sessions is a focus change — not a disconnect/reconnect. Background sessions stay
mounted and keep streaming; the inspector shows live traffic per session; and session-scoped
RPCs target each session's own LiveKit participant.

### Per-session runtime registry

`SessionRuntimeRegistry` (keyed by `sessionId`) replaces the single `useSessionAttachment`
singleton for LiveKit-backed sessions. Each `SessionRuntimeState` holds:

- attachment status
- its own LiveKit `Room` (joined as `browser-{sessionId}-{ts}`)
- its own `GhosttyTerminalLiveKit` instance
- a `ConnectionService` client bound to the session's participant identity
  (`daemon-{instanceId}-{sessionId}`)
- byte counters (in/out), accumulated from the terminal's own I/O events (see below)
- `lastDataReceivedAt` (stamped from inbound terminal output chunks only)
- terminal control state

One `<SessionRuntime>` is mounted per attached session. The focused session's terminal is
CSS-visible; the others are `display:none` but stay subscribed to `streamTerminalIO`.
Selecting a session is a focus switch — no unmount, no `resetAttachment`, no LiveKit
reconnect, no terminal resize.

### Eviction

Background attachments persist until **explicit disconnect** — there is no cap. Disconnect
removes only that session's runtime. Memory therefore grows with the number of concurrently
attached sessions (one LiveKit Room + one Ghostty terminal each); this is intentional for the
fast-switching workflow.

### Session-participant RPC routing

For an attached LiveKit session, the `ConnectionService` client is built via
`liveKitFactory(room, sessionServerIdentity)` where `sessionServerIdentity` is the session's
own participant (`daemon-{instanceId}-{sessionId}`). Session-scoped RPCs route through it:

- `ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`
- `ClaimTerminalControl`, `WatchTerminalControl`
- VNC and screen-sharing RPCs

**Daemon-direct** RPCs stay on the daemon participant (`daemon-{instanceId}`), not the session
participant, so lifecycle and bootstrap control still work when the coder participant is stuck:

- `DeleteSession`, `SignalSession` — lifecycle control, daemon-direct.
- `ConnectSession`, `ResumeSession`, `StartSession` — attachment bootstrap.
- Directory RPCs: `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`,
  `ListEligibleDaemons`, `ListProjectBranches`.

See [Session Participant RPC & Metadata](../coder/session-participant-rpc.md) for the coder
side of this contract.

### Sessions list metadata from participants

`useRoomParticipants`'s `RoomParticipant` carries a parsed `session` metadata field (sibling of
`owned_project_count` / `codex_oauth`, published by the coder process — see
[LiveKit common room: owned project count](livekit-participant-owned-projects.md)).
`SessionManager.mergeActiveAndFetchedSessions` overlays parsed session metadata onto
synthesized cross-host rows and live-updates fetched rows from participant metadata
(presence-driven, no `ListSessions` fan-out for active rows). A common-room participant with
`session` metadata produces a drawer row showing goal/state/agent/model with no `ListSessions`
call for that row.

### Inspector I/O bytes + last-data-received

The inspector **Details** tab shows bytes in, bytes out (both via `formatBytes`, e.g. `1.2 kB`),
and a "last data received: Ns ago" relative timestamp that advances while the inspector is open.
The source is dual:

- **Attached LiveKit session** — the session's `GhosttyTerminalLiveKit` fires an `onBytes` event
  per terminal I/O unit: `bytesIn = output.data.length` per received output chunk, and
  `bytesOut = data.length` per batched input yield sent to the coder. These thread up through
  `SessionLiveKitTerminal` → `SessionRuntime` (`onSessionBytes(sessionId, delta)`) → the screen,
  which folds them into the session's runtime counters via `SessionRuntimeRegistry.recordBytes`
  (`makeByteTap`). The registry's `notify()` re-renders the screen (`useSyncExternalStore`), so the
  meter ticks live — even for a backgrounded session. `lastDataReceivedAt` is stamped from inbound
  chunks only, so sending input (typing/paste) never resets the "Ns ago" clock.
- **No LiveKit participant** — a stopped tddy-coder session, or claude-cli / cursor-cli /
  workspace sessions that never join a room — falls back to `SessionEntry` fields
  (`bytes_in`, `bytes_out`, `last_data_received_at`) populated by the daemon `ListSessions`
  RPC (see [Terminal Sessions § Inspector data for sessions with no LiveKit participant](../daemon/terminal-sessions.md#inspector-data-for-sessions-with-no-livekit-participant)).

The inspector renders the live runtime values when a runtime exists, else the daemon-sourced
`SessionEntry` values. (The screen-level footer's [Session Traffic Strip](#session-traffic-strip)
is a separate, transport-level meter — it counts wire payload bytes, not terminal I/O.)

### Out of scope

- claude-cli / cursor-cli / workspace sessions keep their existing gRPC terminal path
  (`GrpcSessionTerminal`); they have no LiveKit participant and are not bound to the
  runtime registry, background-terminal, or participant-routing behaviour.
- `ConnectionScreen` (`#/`) is unchanged; fast session change affects only
  `SessionsDrawerScreen` (`#/sessions`).

## Session Inspector Drawer

The Session Inspector shows session metadata and action controls in `SessionMainPane`. Its
visibility is controlled by a `data-state` attribute with three values, and its **layout** by a
`data-docked` attribute.

### Docked vs Drawer

`data-docked` is derived from the selected session's connection status via the pure
`isInspectorDocked(session)` helper (`inspectorState.ts`): docked iff
`connectionStatusForSession(session) === "disconnected"`.

- `data-docked="true"` (disconnected session) — the inspector **is the main pane**: when not
  `closed`, it uses the full-pane footprint (`left-0 right-0 w-full`), layered opaque over the
  still-mounted runtime layer behind it. `connected` and `needs-input` sessions are never docked.
- `data-docked="false"` (connected / needs-input) — the inspector is a right-edge overlay drawer
  (the historical behaviour).

All header controls (expand / restore / close) and the header "Inspector" toggle are present in
both layouts.

### Open/Expand State

- `data-state="closed"` — hidden (default for connected sessions)
- `data-state="open"` — as a drawer (`data-docked="false"`), an overlay panel ~360px wide floating
  over the terminal without resizing it; as the main pane (`data-docked="true"`), the full-pane
  footprint
- `data-state="expanded"` — fills the full content area to the right of the session list;
  the session list stays visible so the user can switch sessions

State transitions:
- On session select (connected/active): default `closed`; reset `expanded=false`
- On session select (disconnected/inactive): default `open`; reset `expanded=false`
- Toggle button: `closed → open`, `open → closed`
- Expand button: `open → expanded`
- Restore button: `expanded → open`
- Close button: `open → closed`, `expanded → closed`

### Inspector Header

Title "Inspector", plus expand/restore icon button and close icon button.

### Inspector Metadata Section

All `SessionEntry` proto fields, empty values omitted. Displayed fields:
- Goal (workflowGoal), Status (status), Repo (repoPath), Session ID (sessionId)
- PID (pid, shown when > 0), Workflow state (workflowState)
- Activity status (activityStatus), Agent (agent), Model (model)
- Created (createdAt), Updated (updatedAt)
- Elapsed (elapsedDisplay), Tool (tool)
- Session type (sessionType)
- LiveKit room (livekitRoom)
- Previous session (previousSessionId)

### Inspector Controls Section

Shown below metadata. Actions depend on session state:
- **Resume** — inactive sessions only (ResumeSession RPC)
- **Delete** — all sessions; two-click confirm required (DeleteSession RPC)
- **Terminate** — active sessions only; SIGTERM via SignalSession RPC

## Routing

```typescript
SESSIONS_DRAWER_ROUTE = "/sessions"
isSessionsDrawerPath(pathname)                     // /sessions and /sessions/:id
sessionsDrawerPathForSession(sessionId)            // builds /sessions/<encoded-id>
parseSessionsDrawerSessionId(pathname)             // extracts decoded id or null
```

## UI Primitives

Adds to `src/components/ui/`:
- `tooltip.tsx` — shadcn Tooltip (Radix `Tooltip` namespace)
- `scroll-area.tsx` — shadcn ScrollArea (Radix `ScrollArea` namespace)

## RPCs Used

- `ListSessions` — session list with `isActive`, `createdAt`, `repoPath`, `workflowGoal`, `pendingElicitation`; fanned out per-host and merged (see [Cross-Host Active Sessions](#cross-host-active-sessions); liveness is derived client-side from common-room participants, not from this RPC)
- `ConnectSession` / `ResumeSession` — attach to a running or paused session
- `StreamTerminalOutput` / `SendTerminalInput` — gRPC terminal stream (claude-cli path)
- `DeleteSession` — delete session (two-click confirm)
- `SignalSession` — SIGTERM for active sessions

## Inspector Tabs

The inspector panel has two tabs:

- **Details** (default) — the existing metadata + controls section described above.
- **Tools** — per-session tool-call log and an inline invoke panel.

### Tools Tab

**Call log**: every `ExecuteTool` RPC call made against the session is durably recorded in
`~/.tddy/sessions/{sessionId}/tool-calls.jsonl` (one JSON record per line, append-only).
The call log survives the in-memory `TaskRegistry` eviction (5 min / 200-entry cap) and
daemon restarts. Each row shows:
- Tool name, status pill (ok / error / running), relative timestamp.
- Expandable detail: **Input** (`args_json`), **Output** (`result_json`), **stdio** (for
  `Shell` tool calls, `stdout` / `stderr` / `exit_code` are embedded in `result_json`).

**Invoke panel**: pick a tool from the `ListExecTools` catalog, edit JSON args seeded from
the tool's `input_schema_json`, click Invoke → calls `ExecuteTool` against the session.
Result or error rendered inline. After a successful invoke the call log auto-refreshes.

**Known limitation**: stdio for background Shell jobs (`block_until_ms: 0`) is non-durable.
Live output is accessible via `TaskService.WatchTask` while the task is in the registry
(~5 min). Once evicted, the log row shows only the `job_id` from `result_json`.

### New RPCs

- `ConnectionService.ListSessionToolCalls(session_token, session_id)` — reads the durable
  JSONL log and returns `ToolCallInfo[]` in chronological order.
- `ConnectionService.ListExecTools` — already existed; used to populate the invoke picker.

## Create Session

A **"+ New session"** button in the `SessionDrawer` header opens a creation form in the
main pane. Clicking it switches `SessionsDrawerScreen` into `"creating"` mode; the drawer
remains visible so the user can see existing sessions while filling the form.

### Session Types

A toggle at the top of the form switches between:

- **Tool** — spawns `tddy-coder` via `StartSession` RPC (requires project + agent)
- **Claude CLI** — spawns the Claude Code CLI directly (requires project + model)

### Fields

| Field | Types | Required |
|-------|-------|----------|
| Project | both | yes — dropdown from `ListProjects` |
| Agent/coder | tool | yes — dropdown from `ListAgents` |
| Recipe | tool | no — `<select>` with all 9 workflow recipes (default: `tdd`) |
| PR stack parent | both | conditional — `<select>` listing PR-stack orchestrator sessions; hidden when none exist |
| Base the stack on | tool | conditional — `<select>` naming the existing session that seeds the new stack's bottom node; shown only while the recipe is `pr-stack` (see [Stack Base Session Picker](#stack-base-session-picker)) |
| Model | claude-cli | yes — dropdown of `CLAUDE_CLI_MODELS` |
| Permission mode | claude-cli | no — auto/default/acceptEdits/plan/bypassPermissions |
| Initial prompt | claude-cli | no — textarea |
| Branch mode | both | — New branch from base (optional name + base ref) or Work on existing branch (select from `ListProjectBranches`) |
| Create Remote Branch | claude-cli / cursor-cli | no — **pre-checked** toggle shown in new-branch mode; pushes the new branch to `origin` at session start |

The tool binary (`toolPath`) is auto-selected from `ListTools`; shown as a select only
when multiple tools are available.

> **Updated 2026-07-25** — the new-branch option now reads **"New branch from base: `<name>`"**,
> naming the concrete base ref the branch will be created from (`CreateSessionInitialValues.baseBranchLabel`).
> A **"Create Remote Branch"** checkbox (`create-session-create-remote-branch-toggle`, shown only for
> `claude-cli` / `cursor-cli` session types in `new_branch_from_base` mode, **default checked** via
> `initialValues?.createRemoteBranch ?? true`) sends `StartSessionRequest.create_remote_branch` (proto
> field 28). When set, the daemon runs `git push -u origin <branch>` after worktree creation
> (`tddy_core::worktree::push_new_branch_to_origin`) and records `Changeset.remote_pushed = true`; a
> push failure fails session start (no fallback).

> **Updated 2026-07-26** — `CreateSessionInitialValues.selectedBranch` pre-fills the **Work on existing
> branch** select, honoured only when `branchIntent === "work_on_selected_branch"`. It is what lets a
> caller open the dialog pre-set to *resume* a branch that already exists instead of creating one — see
> [§ PR-Stack Chat Screen § Start session CTA](#start-session-cta).

### Recipe Dropdown

The recipe `<select>` lists all 8 workflow recipes (constant `WORKFLOW_RECIPES` in
`CreateSessionPane.tsx`): `tdd`, `tdd-small`, `bugfix`, `free-prompting`, `grill-me`,
`review`, `merge-pr`, `pr-stack`. Defaults to `tdd`.

> **Updated 2026-07-01** — the legacy `plan-pr-stack` and `orchestrate-pr-stack` entries were
> consolidated into a single `pr-stack` recipe (see
> [PR stacking § pr-stack recipe](../coder/pr-stacking.md#pr-stack-recipe)). Both legacy CLI
> names still resolve on the backend for back-compat, but the dropdown only offers the
> canonical `pr-stack` name for new sessions.

### PR Stack Parent Picker

When creating a **tool** or **claude-cli** session, the form also calls `ListSessions` and
filters to PR-stack orchestrator sessions — sessions with `recipe === "pr-stack"` (or a
legacy alias) that are not themselves children of another orchestrator. This filtering
is performed by the `prStackOrchestrators()` helper in `src/utils/stackParents.ts`.

The `recipe` field is populated on `SessionEntry` (proto field 22) by the daemon's enrichment
layer from `Changeset.recipe`; the TS filter reads it directly without reverse-deriving from
child back-references (the old `stackParentCandidates` approach).

If any candidates exist, a **PR stack parent** `<select>` appears. Selecting a session causes
`StartSession` to include `stackParent = <session_id>` (proto field 15).

For **tool** sessions: the daemon threads `stackParent` as `--stack-parent <id>` to the
spawned `tddy-coder` process, setting `Changeset.orchestrator_session_id` on the child.

For **claude-cli** sessions: the daemon writes `orchestrator_session_id` into the child's
`changeset.yaml` directly, and resolves the parent's branch via
`resolve_chain_integration_base_ref_from_parent_session` so the child worktree is based off
`origin/<parent-branch>` (git PR-stack chaining).

### Stack Base Session Picker

> **Added: 2026-08-13** — a new `pr-stack` orchestrator can be created **on top of a session that
> already exists**, instead of leaving the agent to plan a stack from a feature description.

A **"Base the stack on"** `<select>` (`create-session-pr-stack-base-session-select`) appears when the
session type is **tool** and the recipe is `pr-stack`. It reads the same
`ListSessions` fetch as the [parent picker](#pr-stack-parent-picker), filtered by
`stackBaseSessionCandidates()` in `src/utils/stackParents.ts` to the sessions that **own a branch**, are
in the form's effective project, are on its effective host, and are not already a node of another
orchestrator's stack. Options read `<session id> — <branch>`; the default option is empty and reads
*"None (agent plans the stack)"*.

Choosing one sends `StartSessionRequest.pr_stack_base_session_id` (field 31), and the created
orchestrator comes up with a one-node stack bound to that session's branch and id, already in its
`orchestrate` operator loop. Switching project or host clears the choice, and the field is only ever
sent while the recipe is still `pr-stack`.

The daemon validates the choice **before it spawns anything**, so a base session that cannot seed a
stack (no branch, unresolvable, another repository, already owned by another orchestrator) is reported
in the form's own error strip and nothing is created. Rules, refusals and the seeded node's contents:
[PR stacking § Seeding the stack from an existing session](../coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13).

### Post-Create

On success, `SessionsDrawerScreen` navigates to `/sessions/:newId` and auto-attaches
(same behaviour as clicking an active session in the drawer).

A creation refused because another session already owns the requested branch does **not** navigate:
the response carries `branch_conflict`, and the form opens a three-choice prompt (switch to the
owning session / add a second agent on that branch / name a different branch) instead of silently
creating a `<branch>-1` suffixed branch. See
[Session Branch Conflict](../daemon/session-branch-conflict.md).

### Component

`CreateSessionPane` (`packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`) — props:

```typescript
interface CreateSessionPaneProps {
  client: ConnectionClient;
  sessionToken: string;
  onCancel: () => void;
  onCreated: (sessionId: string) => void;
}
```

### RPCs Used (Create Session)

- `ListProjects` — project dropdown
- `ListTools` — auto-select tool binary
- `ListAgents` — agent dropdown (tool sessions)
- `ListSessions` — populate the PR stack parent picker and the stack-base picker from one fetch (best-effort; failure hides both)
- `ListProjectBranches` — branch dropdown when "work on existing branch"
- `StartSession` — create + start the session

## PR-Stack Session Grouping

> **Added: 2026-06-26** — Sessions that are children of a PR-stack orchestrator are now
> displayed nested under their orchestrator in the drawer.

### Data Model

`SessionEntry.orchestratorSessionId` (proto field 21, `string`) carries the back-reference
from a child session to its PR-stack orchestrator. The daemon populates it from
`Changeset.orchestrator_session_id` via `session_list_enrichment.rs`. Empty string for
non-child sessions.

### Grouping Logic

`groupSessionsByStack(sessions)` (in `src/utils/sessionStackGroups.ts`) partitions the
session list:

- **Group** — an orchestrator session paired with one or more children that reference it.
  Children sorted oldest-first by `createdAt`.
- **Flat** — plain sessions (no `orchestratorSessionId`) and orphan children whose
  orchestrator is not in the current list.

Groups are sorted newest-first by the orchestrator's `createdAt`.

### Drawer Rendering

`SessionDrawer.tsx` renders:

```
<details data-testid="sessions-drawer-stack-{orch-id}" open>
  <summary>
    <SessionDrawerItem session={parent} />     ← orchestrator row
  </summary>
  <SessionDrawerItem session={child} depth={1} />  ← each child, indented
</details>
```

- `open` attribute: groups start expanded.
- Clicking `<summary>` collapses/expands the group (native browser `<details>` behaviour).
- `SessionDrawerItem` uses the `depth` prop to set `data-depth` and left-padding, giving
  child sessions a visual indent.
- Orphan children (orchestrator absent from list) appear in the flat section without a group.

### Stack Parent Picker

See [PR Stack Parent Picker](#pr-stack-parent-picker) in the Create Session section above.

## Session Agents (Peer Agent Sessions)

A session detail view (`SessionMainPane`) lists the selected session's **peer agent sessions** —
child sessions linked back to it by `SessionEntry.orchestratorSessionId` (the same field the
PR-stack grouping above uses), each running its own agent backend. The operator switches between
them from the session detail view. There is **no agent-to-agent messaging** — the operator
coordinates them.

> **Updated 2026-08-29 — two different things are called "agent" in this pane, and the header
> button now means the second one.** A **peer** is a whole session, and it arrives from the *agent*
> side: a managed workflow calling [`spawn_conversation`](../coder/spawn-conversation.md), or a
> PR-stack orchestrator spawning a child for a planned PR
> ([PR stacking](../coder/pr-stacking.md)). A **roster agent** is not a session at all — it is a
> specialized agent attached to *this* session
> ([session agent roster](../daemon/session-agent-roster.md)), and it never appears in the peer
> list. The **Add agent** button used to produce the first and now produces the second: the
> peer-spawn flow it drove, its `#/sessions/:id/add-agent` route and `CreateSessionPane`'s peer mode
> are all gone. The two are easy to confuse, so the distinction is worth holding onto — nothing the
> header button does adds a row to the peer list below.

### Add agent

`SessionMainPane`'s header carries an **"Add agent"** button
(`data-testid="session-agent-attach-btn"`, a peer of the `Code` / `Inspector` / activity-overlay
toggles). It **attaches a roster agent to the session on screen** and opens a conversation tab with
it — it does not create a session. The button is only rendered when the pane holds a daemon client:
there is no attach without one, and offering a control that cannot act is worse than not offering
it.

The flow is picker → attach → tab:

1. **Pick.** Clicking the button opens `AgentPicker` (`session-agent-picker`) below the header —
   the same component the Inspector's Agent roster pane mounts, extracted so there is exactly one
   picker and an operator is never told two different things about the same catalog. It is a
   **fan-out**: `ListSubagents` carries no routing field and a daemon answers only for its own defs,
   so the picker reads its home host through the app's transport and addresses every other
   common-room daemon over LiveKit RPC (`useAvailableAgents` over `useHostFanOut`). One host failing
   to answer costs one error row (`session-agent-picker-host-error-<daemonInstanceId>`) and leaves
   the other hosts' agents on offer.
2. **See the cost, then confirm.** Each option
   (`session-agent-picker-option-<agentId>`, with the owning host beside it under
   `…-option-<agentId>-host`) is offered under its **qualified** `name@daemon_instance_id` — two
   hosts routinely offer a def called `explorer`, and only the qualified id says which one. Picking
   one states what the main agent loses (`session-agent-picker-withdrawal-warning`): every tool in
   the def's `replaces` list stops being callable by the main agent for as long as the agent stays
   attached. **Attach** (`session-agent-picker-confirm-btn`) confirms; **Cancel**
   (`session-agent-picker-cancel-btn`) dismisses without attaching.
3. **Attach.** Confirming calls **`AttachSessionAgent`** with the current session's `sessionId` and
   its facilitating `daemonInstanceId`, and the picked agent's qualified `agentId`. The facilitating
   daemon runs a local roster entry in-process and forwards a remote one to its owning host, so the
   web never has to know which host answered. A refusal is rendered in the picker
   (`session-agent-attach-error`) and the picker stays open; only a successful attach closes it. The
   confirm control is disabled while an attach is in flight, because whether a second tab is needed
   is decided from the state visible when the attach *resolves* — two attaches of the same agent
   racing would both find no conversation open.
4. **Talk.** A successful attach mints a `conversation_id`, adds a **conversation tab** to that
   session's [tab strip](session-terminal-tabs.md) and focuses it. The id is minted in the browser
   with `randomUuid` (`src/lib/randomId.ts`) and never `crypto.randomUUID`, which is `undefined` on
   the plain-http LAN origins this app is routinely served from; `OpenAgentConversation` accepts a
   caller-chosen id precisely so the caller can still name — and therefore cancel — what it opened.

Attaching an agent that already has a tab open **focuses that tab** instead of opening a second one.
A repeat `AttachSessionAgent` is a no-op on the roster, so a second tab would claim something the
daemon did not do.

#### The conversation tab

| Element | Test id |
|---|---|
| Tab | `sessions-agent-tab-<conversationId>` |
| Tab close control | `sessions-agent-tab-<conversationId>-close` |
| Tab body (mounted pane) | `sessions-agent-pane-<conversationId>` |
| Transcript | `agent-conversation-transcript` |
| One turn | `agent-conversation-turn-<index>` |
| Composer input | `agent-conversation-input` |
| Send | `agent-conversation-send-btn` |
| Failure line | `agent-conversation-error` |

Tabs are keyed by the **conversation** id, not the agent id: an agent can be attached with no
conversation open, and closing a tab cancels a conversation, not an attachment. The close control is
a *sibling* of the tab button rather than a child — nesting it would make closing a conversation
also a request to look at it, and would put a ✕ inside the tab's accessible name.

The body is `SessionAgentConversationPane`, driven by the `useAgentConversation` hook:

- **`OpenAgentConversation` is issued by the tab's body, not by the header.** One owner for the
  conversation's whole life is what keeps exactly one open per tab — the header opening it as well
  would open every conversation twice — and it is what makes a failed open surface in the tab that
  holds it rather than in a picker that has already closed.
- Sending a prompt calls **`PromptAgentConversation`**. Its `content_chunk` frames accumulate into a
  single agent turn; the final frame's `stop_reason` closes it. A turn carries `data-role`,
  `data-complete` and `data-stop-reason`, and the speaker's name is rendered as a sibling of the
  turn rather than inside it, so an **empty answer reads as an empty completed turn** — the daemon
  guarantees exactly one frame, and "said nothing" must never render as "nothing arrived".
- The composer is closed while an answer is still arriving. A second prompt sent into a live stream
  would strand itself mid-answer and merge two answers into one turn. The gate is in the send
  handler, not on the button, because the button is not the only way in — Enter is.
- A failed open and a failed prompt are each named on their own line. Neither is ever shown as an
  empty transcript, and a failed prompt keeps the operator turn that provoked it.
- Closing the tab unmounts the body, which calls **`CancelAgentConversation`**, and focus returns to
  the Agent terminal tab. The cancel waits on the open it is cancelling: landing first would have
  the daemon answer `NOT_FOUND` for a conversation it had not created yet, and the open would then
  land behind it, leaving an agent session running with nothing left to cancel it.
- Switching tabs or sessions tears nothing down. Every open conversation's body stays mounted and is
  hidden by `display: none`; the visible one is positioned `absolute inset-0` at `z-index: 3` over
  the terminal, stated inline because the terminal states its own stacking inline too and a
  conversation under a still-painting terminal is one the operator cannot type into.

The open conversations and the focused one are held **in `SessionMainPane`**, keyed by session id,
rather than inside `SessionRuntime`: a runtime is backgrounded and not unmounted on a session
switch, and must come back with its conversations intact.

This is a live conversation, never a replay. It is deliberately **not** the Agent Activity overlay
or the [agent activity pane](agent-activity-pane.md), which replay a *session's* recorded ACP
transcript: a roster agent has no session directory and no transcript, so the main agent's own use
of its sub-agents stays visible only as the roster row's status badge and last-activity line.

### Session agents section

`SessionAgentsSection` (`src/components/sessions/SessionAgentsSection.tsx`) mounts below the
header and lists the selected session's peers — sessions with
`orchestratorSessionId === selectedSession.sessionId`, derived by the pure
`sessionPeers(sessions, currentSessionId)` util (`src/utils/sessionPeers.ts`). Each row shows the
peer's `sessionId` / `agent` / `model` / `status` and a **switch** button
(`data-testid="session-agents-switch-<peerSessionId>"`) that selects the peer in the drawer
(focusing its runtime). An empty-state message is shown when there are no peers, so a session
without peers sees no list noise.

> A peer and a PR-stack child are indistinguishable in the section today (both are children via
> `orchestratorSessionId`). A future filter could distinguish by recipe.

### Reused infrastructure

- `SessionEntry.orchestratorSessionId` (proto field 21) — the back-reference from a child to its
  orchestrator, which is the whole of how the peer list is discovered: no new RPC, just the
  `ListSessions` poll the drawer already runs.
- `SessionsDrawerScreen.onChildSessionStarted` — the optimistic drawer overlay, so a child spawned
  from the PR-Stack screen appears without waiting for the next poll.
- `AttachSessionAgent` / `OpenAgentConversation` / `PromptAgentConversation` /
  `CancelAgentConversation` — the roster and conversation RPCs behind **Add agent**, all of which
  already existed and were already reachable with a `session_token`.

No proto or daemon changes; no new external dependencies. Both halves are tddy-web-only.

## Per-Workflow Session Views

> **Added: 2026-07-01** — a session's `recipe` can now select a fully custom main-pane
> screen instead of the terminal. First consumer: the PR-Stack Chat Screen below.
>
> **Updated: 2026-07-21** — chat is now the default surface for **every** tddy-coder workflow.
> `pr-stack` keeps its dedicated screen (chat + Planned PRs panel); every other `tool` workflow recipe opens the
> single-pane full-screen [Workflow Chat Screen](#workflow-chat-screen). `claude-cli` / `cursor-cli`
> PTY sessions keep the terminal (they have no Presenter).

### View registry

`SessionMainPane.tsx` gains a resolution step before the existing
`isConnected ? <terminal> : <placeholder>` branch:

```typescript
const customView = resolveWorkflowView(selectedSession);
if (customView) return customView;
```

`resolveWorkflowView(session)` (`src/components/sessions/workflowViews.tsx`) is a small
registry keyed by `session.recipe` and `session.sessionType`:

```typescript
resolveWorkflowView({ recipe: "pr-stack", ... })              → <PrStackScreen session={...} />
resolveWorkflowView({ recipe: "tdd", sessionType: "tool" })   → <WorkflowChatScreen session={...} />
resolveWorkflowView({ recipe: "tdd", sessionType: "claude-cli" }) → null  // terminal (PTY, no Presenter)
resolveWorkflowView({ recipe: "", sessionType: "claude-cli" }) → null      // terminal
```

The gate for the generic chat screen is `sessionType ∈ {"", "tool"} && recipe != ""` — only
tddy-coder `tool` sessions run a Presenter/ACP surface the chat can reach, so a `claude-cli` /
`cursor-cli` session keeps the terminal even when it carries a managed `recipe`. `pr-stack` is
matched first and routes to its own chat + Planned PRs panel screen.

Custom views own their own connection/chrome and render **in place of** the terminal
container — they are not gated on `attachment.status`; a workflow session shows its chat screen
whether or not a terminal is attached, since the whole point is that the operator never needs the
raw terminal for these workflows. Non-tool sessions are unaffected: the existing terminal /
placeholder behaviour is the fallback when no custom view is registered.

### Data source

`SessionEntry.recipe` (proto field 22) and `SessionEntry.sessionType` (proto field 17), both
already surfaced for the create-session form / inspector, are the routing keys — no new field was
needed to decide *which* view opens.

## PR-Stack Chat Screen

The custom screen for `recipe === "pr-stack"` sessions. Replaces the terminal with a **full-width chat
window** backed by a remote Presenter, plus a **Planned PRs panel** docked to its right. Lets the
operator review a freshly-written stack plan, keep refining it by chatting with the agent, and start
child sessions for each planned PR — all without leaving the orchestrator session.

**Component:** `PrStackScreen` (`src/components/sessions/prstack/PrStackScreen.tsx`), with
`PlannedPrPanel` / `PlannedPrList` / `PlannedPrRow` subcomponents.

> **Rewritten 2026-07-26** — the screen was a fixed 50/50 split (`w-1/2` columns, no toggle, no
> breakpoint handling), which permanently halved the chat on desktop and was unusable on mobile with no
> way to dismiss the list. The list is now a panel with the same contract as the Session Inspector.

### Planned PRs panel

| Viewport | Default | Layout |
|---|---|---|
| Desktop (≥ 768px) | **open** | docked 360px column to the right of the chat |
| Mobile (< 768px) | **closed** | full-screen overlay above the chat |

- **Always mounted**; `data-state` ∈ `{closed, open}` drives visibility
  (`data-testid="pr-stack-planned-pr-panel"`), matching `SessionInspectorDrawer` — so the list keeps its
  scroll position and the screen keeps its branch poll set across a close and reopen.
- Positioned `absolute top-0 right-0 z-10` against the screen's **content row** (which carries
  `relative`), not the screen root — so the panel never covers the header toggle that opens it. The
  chat owns the full width and reserves a `paddingRight` column only while the panel is docked; the
  mobile overlay deliberately covers the chat instead.
- **Width and closed-visibility are inline styles driven by an `isMobile` prop**, not `w-full
  md:w-[360px]` + a Tailwind `hidden` class. They are the panel's layout contract rather than
  decoration, and `SessionDrawer` already establishes the idiom: component tests mount without the app
  stylesheet, so a media-query/class-only panel would have no width and no way to hide.
- Two controls: a header **toggle** in the screen's own top strip
  (`pr-stack-planned-pr-panel-toggle`, labelled "Planned PRs") and the panel's own **close** control
  (`pr-stack-planned-pr-panel-close`). The open/closed seed is `detectIsMobile()`; live changes come
  from `useIsMobile()`.
- The panel contains the planned-PR list, the "+ New planned PR" entry point, and the add form.

### Planned-PR list

Reads the orchestrator's `Stack` (see [PR stacking § Stack data model](../coder/pr-stacking.md#stack-data-model))
via `SessionEntry.stackPlanJson` (proto field 23 — a JSON-serialized `Stack`, empty string
until a plan exists) and renders one row per `StackNode`.

> **Updated 2026-08-01** — rows render in the order the plan **persists** (`StackNode.display_order`),
> not in `Stack::topo_order`. The reading order and the dependency graph are different facts, and
> deriving one from the other meant a merge, a repoint or a re-parenting silently moved a row the
> operator was reading. `orderStackNodes` falls back to `topoSortStackNodes` **wholesale** for a plan
> that carries no positions at all (a plan authored before this existed); a half-numbered plan takes
> the same fallback, since interleaving real positions with invented ones can render a child above its
> parent. See [PR-Stack live status § Persisted display order](../coder/pr-stack-live-status.md#persisted-display-order).

**Row anatomy** *(2026-08-01)* — a row is a collapsed summary that expands to its full detail:

| Region | Contents | Visibility |
|---|---|---|
| Summary header | the toggle (`pr-stack-row-toggle-<nodeId>`, carrying the title and `aria-expanded`), the badge strip, and the CTA slot | always |
| Detail body (`pr-stack-row-details-<nodeId>`) | description, branch / planned branch, base branch, worktree, node id, parents, child recipe, child state, bound child session, conflicted paths, and the reorder + pull controls | **hidden, never unmounted** |
| Footer | `pr-stack-start-warning-<nodeId>`, `pr-stack-repoint-error-<nodeId>`, `pr-stack-sync-error-<nodeId>`, `pr-stack-reorder-error-<nodeId>` | always |

The detail is hidden with `display:none` rather than unmounted, so expansion, scroll position and the
branch poll set all survive a collapse — and every existing information contract keeps holding. Errors
and blockers sit **outside** the collapse boundary: a reason the operator must expand a row to find is
the dead end D16 exists to remove. The badge strip and CTA are siblings of the toggle, never nested
inside it — a button within a button would swallow the Start-session click.

The detail carries the node's **branch name** (`pr-stack-branch-<nodeId>`) or, when no branch exists yet,
its `branchSuggestion` marked as **planned** (`pr-stack-planned-branch-<nodeId>`, rendered as
`planned: <name>`) — a suggestion names no ref, so the two are never shown the same way — plus the live
**worktree** (`pr-stack-worktree-<nodeId>`) and the **base branch** its child worktree would be created
from (`pr-stack-base-branch-<nodeId>`), whatever its startability. The summary strip carries the
**in-progress** badge, the **PR** link/state (`pr-stack-pr-link-<nodeId>` / `pr-stack-pr-state-<nodeId>`)
or **"PR status unavailable"** (`pr-stack-pr-unavailable-<nodeId>`, reason as its `title`), and the
base-sync badge below. All of it is resolved by branch through the `QueryBranch` RPC (`useQueryBranch`,
per-branch polled). See
[PR-Stack live status § QueryBranch](../coder/pr-stack-live-status.md#api-surface).

**The spawned indicator opens its session** *(2026-08-01)*. When the node's bound child session resolves,
the status chip is **wrapped** (not replaced) in `pr-stack-session-<nodeId>`, which selects and attaches
that session exactly as clicking it in the drawer does. Resolution prefers the session the plan records
and falls back to the branch's current owner, each guarded on the session being one the drawer knows; when
neither resolves the chip stays plain text, because a control that selects nothing is worse than no
control. Navigation is in-app — the PR link remains the row's only new-tab affordance.

The URL follows. `onOpenSession` is wired to the drawer's own `handleSelectSession`, which since
[#374](url-state-routing.md) navigates rather than only setting state — so opening a session from the
panel is URL-addressable and back-button-able for free, with no PR-stack-specific routing. (This was
a documented gap when the panel shipped; the drawer-wide change that closed it landed the same day.)

**Base-sync badges** *(2026-08-01)*, in the summary strip, mutually exclusive:

| State | Badge |
|---|---|
| Behind by N commits, no conflicts | `pr-stack-base-behind-<nodeId>` — "N behind `<base>`" |
| Would conflict | `pr-stack-base-conflicts-<nodeId>`; the paths in the detail (`pr-stack-base-conflict-paths-<nodeId>`) |
| Contains every commit on its base | `pr-stack-base-in-sync-<nodeId>` |
| Comparison could not be made | `pr-stack-base-sync-unavailable-<nodeId>`, reason as its `title` |
| Poll has not answered / older daemon | nothing |

An unavailable comparison is **never** rendered as clean, and "in sync" is a badge rather than silence —
otherwise a healthy row and a row whose poll has not answered would look identical.

**Row controls in the detail** *(2026-08-01)*:

- **Reorder** — `pr-stack-move-up-<nodeId>` / `pr-stack-move-down-<nodeId>`, disabled at the ends of the
  rendered order and while a reorder is in flight, backed by `ReorderPlannedPr`.
- **Pull from base** — `pr-stack-sync-merge-<nodeId>` ("Merge N commits from `<base>`", singular at 1) and
  `pr-stack-sync-rebase-<nodeId>`, backed by `PullBaseIntoBranch`. Offered **only** when the row is cleanly
  behind and owns a branch: not at zero commits, not on conflicts, not on an unavailable or unarrived
  comparison. Both disable together while either runs — a concurrent merge and rebase of one branch is
  destructive, not merely wasteful. Merge is the default; rebase is the operator's explicit choice.
- **Dirty worktree** — when the branch's worktree holds uncommitted tracked changes, clicking a pull
  control opens `pr-stack-dirty-worktree-dialog` naming those paths, offering to commit and push them
  first. Cancelling calls nothing at all.

**The CTA slot holds exactly one of two things**, and a blocked row carries its warning *beside* the
CTA rather than in place of it:

| Node condition | CTA slot | Warning |
|---|---|---|
| No `session_id`, base branch present on `origin` (or the node is a root) | **"Start session"** button (`pr-stack-start-session-<nodeId>`), enabled | — |
| No `session_id`, base branch absent from `origin` / no ancestor branch / a branchless non-merged parent | the same button, **disabled**, with the reasons as its `title` | `pr-stack-start-warning-<nodeId>`, one line per blocking reason |
| `session_id` set, and the branch resolution has not arrived or reports a session | status chip (`pr-stack-status-chip-<nodeId>`) from `pr_status.phase` / `child_state` | — |
| `session_id` set, and the resolution reports **no** session (orphaned) | **"Start session"** button again, pre-filled to resume the node's branch | — |

> **Added 2026-07-26** — a node that already owns a branch is never blocked (its spawn resumes that
> branch and fetches nothing), and a base whose resolution has not arrived is *unknown*, never missing.
> `isNodeOrphaned` / `branchlessNonMergedParent` / `resolveStackBase` are the pure modules behind those
> states; see
> [PR-Stack live status § Startability before the spawn](../coder/pr-stack-live-status.md#startability-before-the-spawn-added-2026-07-26).

> **Revised 2026-07-26** — the blocked state no longer **replaces** the row's contents. It used to
> render a `pr-stack-missing-branch-<nodeId>` indicator in place of the button, which cost the operator
> the CTA and gave them no action; that test id is **gone**. The row now keeps everything it has, the
> button is disabled with its reasons, and a **Repoint** control sits beside it — see
> [PR-Stack live status § Repointing a dead-end planned PR](../coder/pr-stack-live-status.md#repointing-a-dead-end-planned-pr-added-2026-07-26).
> The Repoint control reads **"Repoint to `<target>`"** (`pr-stack-repoint-<nodeId>`), is disabled while
> its call is in flight, and reports a refusal inline as `pr-stack-repoint-error-<nodeId>`.

**"+ New planned PR" can start the node's session too** *(2026-08-13)*. The add form
(`pr-stack-add-planned-pr-form`) carries two submits: **"Add planned PR"**
(`pr-stack-add-planned-pr-submit-btn`), which only appends the node, and **"Add & start session"**
(`pr-stack-add-planned-pr-start-btn`), which appends it and then opens the same
[Start session](#start-session-cta) dialog the new row's CTA would open — pre-filled identically —
so adding a PR and starting work on it is one action rather than a hunt for the row that was just
added. The node is identified by `AddPlannedPrResponse.node_id`, named by the daemon rather than
inferred by diffing the returned plan: the orchestrator agent appends to the same stack, so one
response can carry several ids the panel has never seen. A response whose plan does not contain the
node it named still updates the list — the node was appended — but starts nothing and says so in the
form's error strip. See
[PR stacking § Add and start in one step](../coder/pr-stacking.md#add-and-start-in-one-step-added-2026-08-13).

### Start session CTA

Clicking "Start session" on a node opens the shared **`CreateSessionDialog`** — the
same `CreateSessionPane` form the sessions drawer uses — **pre-filled** from the node and its
orchestrator, so the operator can review and adjust before spawning:

- `projectId` and host (`daemonInstanceId`) come from the orchestrator session.
- `stackParent` = this orchestrator session's id; `sessionType` = `"claude-cli"`.
- Branch mode depends on whether the node already **owns** a branch *(2026-07-26)*:
  - **no branch** → `new_branch_from_base`, `newBranchName` = the node's `branchSuggestion`.
  - **owns a branch** (the orphaned-recovery case) → `work_on_selected_branch` with
    `CreateSessionInitialValues.selectedBranch` = that branch, so the session **resumes** it. The
    branch, its worktree and its remote ref outlive the session that made them, so
    `new_branch_from_base` would fail on "branch already exists" — which is exactly the node this
    recovery path exists for. `CreateSessionPane` honours `selectedBranch` only in that mode.
- `baseBranchLabel` = the node's concrete base branch — derived from its stack position by
  `deriveStackBaseBranch(node, nodes, defaultBranch)` (the nearest non-merged ancestor that owns a
  **created `branch`**, collapsing to the project default for a root or all-merged node) — so the
  dialog reads **"New branch from base: `<predecessor branch>`"**. A parent holding only a
  `branchSuggestion` is passed over like an absent one: a suggestion names no ref, so previewing it
  would promise a base the daemon's branch-gated spawn then refuses.
- `createRemoteBranch` is pre-checked, so submitting also pushes the new branch to `origin` (see
  [Create Session § Fields](#fields)).
- Initial prompt = the node's title + description.

Submitting issues the existing `ConnectionService.StartSession` RPC (no new RPC surface — `recipe`
and `stack_parent` were already added for the [parent picker](#pr-stack-parent-picker) in #246) and
fires `onChildSessionStarted` so the drawer optimistically shows the child row.

The daemon's existing chain-base-ref resolution (`resolve_chain_integration_base_ref_from_parent_session`)
derives the child's base branch from the node's *parents* in the stack — `origin/<parent-branch>`
when the node has an unmerged parent, collapsing to the default branch once all parents are
merged (`effective_base_ref`, [PR stacking § assess decision algorithm](../coder/pr-stacking.md#assess-decision-algorithm-priority-order)).
After the child is spawned, the node is linked via `link_stack_node_to_child_session` and the
row updates from CTA to status chip on the next session-list refresh.

### Chat window (remote Presenter over RPC)

The chat surface (full-width behind the Planned PRs panel) is a thin UI over the session's existing **remote Presenter**
protocol — the same bidirectional `TddyRemote.Stream` RPC already used for programmatic
control (`tddy-service/proto/tddy/v1/remote.proto`), not a new backend concept:

- **Inbound** — the screen subscribes to the stream over the session's LiveKit room
  (`useLiveKitClient(TddyRemote)`), and renders each `ServerMessage` `PresenterEvent` as a
  chat item: `AgentOutput` / `ActivityLogged` → agent bubbles, `StateChanged` / `GoalStarted`
  → status lines, `WorkflowComplete` → a completion bubble.
- **Outbound** — submitting chat text sends a `ClientMessage` intent on the same stream:
  `QueuePrompt` for a plan-refinement turn, `AnswerSelect` / `AnswerText` for clarification
  answers.
- **Refine loop** — a refinement turn causes the recipe's `write-stack-plan` goal to re-run
  (`plan_refinement_goal()`, [PR stacking § pr-stack recipe](../coder/pr-stacking.md#pr-stack-recipe)),
  which rewrites `stack-plan.yaml` and re-seeds `Changeset.stack`. The planned-PR list
  re-reads `stackPlanJson` after a `WorkflowComplete` / `StateChanged` event so edits appear
  without a manual refresh.

**Component:** the reusable **`AgentChat`** (`src/components/chat/AgentChat.tsx`) +
`useAgentChat` hook — see [Agent Chat](#agent-chat) below. `PrStackScreen` mounts `AgentChat`
with a pr-stack-appropriate placeholder; the component itself is recipe-agnostic.

## Workflow Chat Screen

The custom screen for every non-`pr-stack` tddy-coder `tool` workflow session (`tdd`, `tdd-small`,
`bugfix`, `free-prompting`, `grill-me`, `review`, `merge-pr`). Replaces the terminal with a single,
full-screen chat window — no second pane. The operator drives the workflow entirely by prompting and
answering clarifications, exactly as with pr-stack, minus the planned-PR list.

**Component:** `WorkflowChatScreen` (`src/components/sessions/WorkflowChatScreen.tsx`) — a thin
single-pane wrapper that derives its own presenter LiveKit room from the session's attachment
(`usePresenterLiveKitRoom`, shared with `PrStackScreen`) and mounts the reusable [`AgentChat`](#agent-chat)
over the **ACP mirror** (`acp`), matching pr-stack's chat and tddy-coder's ACP-agent direction. Root
marker: `data-testid="workflow-chat-screen"`.

## Agent Chat

`AgentChat` is the recipe-agnostic chat window extracted from the PR-Stack screen. It is a thin
UI over a session's remote agent and knows nothing about PR stacks — any recipe can mount it. It
speaks one of two wire protocols over the same LiveKit session connection, chosen by an `acp` prop:
the default Presenter `TddyRemote.Stream`, or the ACP protobuf mirror `AcpService.Session` (see
[acp-protobuf-rpc](../coder/acp-protobuf-rpc.md)). **The pr-stack chat uses ACP** (`acp`), at full
behavior parity with the Presenter path.

- **Inputs:** `room: Room | null` + `livekitServerIdentity` select the LiveKit transport target;
  `placeholder` is display-only; `acp` selects the ACP transport. There is no dependency on
  `SessionEntry` or any pr-stack type.
- **Behavior:** inbound agent output chunks are merged into one growing agent bubble (mirroring
  the TUI's `AgentOutputActivityLogMerge`); a select / multi-select clarification renders a
  clarification panel; outbound text starts (first message on a fresh connection) or nudges the
  workflow.
- **Export:** an "Export" button downloads a plain-text transcript with ISO timestamps, merging
  chat messages and clarification (elicitation) points into one chronological timeline
  (`chatTranscript.buildChatTranscript` + `downloadTextFile`) — so an operator can see what the
  agent did and when, including where it paused for input. Works on both transports.
- **Test ids:** `agent-chat-*` (e.g. `agent-chat-messages`, `agent-chat-message-<i>`,
  `agent-chat-input`, `agent-chat-option-<i>`, `agent-chat-export-btn`), centralized in
  `cypress/support/testIds.ts`. Storybook: `AgentChat.stories.tsx` (empty / streaming / select /
  multiSelect / error / connecting).

**Hooks:** `useAgentChat(room, serverIdentity)` owns the `TddyRemote.Stream` bidi RPC;
`useAcpSession(room, serverIdentity)` is its ACP counterpart over `AcpService.Session`. Both return
the identical `UseAgentChatResult` — `messages`, `elicitations`, `sendPrompt`, `pendingQuestion`,
`answerSelect` / `answerOther` / `answerMultiSelect`, and the `streamError` / `sendError` /
`workflowError` surfaces — so `AgentChat` renders either through a shared `AgentChatView`.

### New RPCs / proto fields used

- `SessionEntry.stack_plan_json` (proto field 23, new) — JSON-serialized `Stack` for the
  planned-PR list.
- `TddyRemote.Stream` (existing) — chat window transport.
- `ConnectionService.StartSession` (existing, `recipe` + `stack_parent`) — the Start session CTA.

## Session Traffic Strip

> **Relocated: 2026-07-21** — The traffic readout has moved into the screen-level bottom
> **Host Stats Footer** (see [host-stats-footer.md](./host-stats-footer.md)), alongside new
> host-level disk and CPU indicators. It is no longer rendered in the top header row.
>
> **Data-plane rescoped: 2026-07-24** — The data-plane half of the readout now aggregates the
> per-session terminal byte tap across **every attached runtime** (focused + backgrounded), not
> just the focused session's LiveKit room. See § Metering scope below.

A thin `flex-shrink-0` strip showing live RPC throughput and connection health for the
selected session.

### Display

The strip shows five values:

| Field | Description |
|-------|-------------|
| ↓ rate | Live inbound throughput in B/s (or kB/s, MB/s) averaged over the last ~2 s |
| ↑ rate | Live outbound throughput |
| ↓ total | Cumulative session bytes received |
| ↑ total | Cumulative session bytes sent |
| Ping | Round-trip time to the LiveKit gateway in ms, or `—` when unavailable |

### Metering scope

Two planes are metered independently and summed for display:

- **Data plane (session terminal I/O)** — the per-session byte tap (see
  [§ Inspector I/O bytes](#inspector-io-bytes--last-data-received)) **aggregated across every
  attached runtime**, focused and backgrounded. `useAttachedSessionTraffic(runtimes,
  runtimeRegistry)` sums each mounted runtime's cumulative `bytesIn`/`bytesOut` for the totals and
  folds their advances into one shared `TrafficMeter` for the aggregate rate. Because backgrounded
  runtimes keep streaming, a session left in the background still contributes; the total is **not**
  reset when focus moves between sessions.
- **HTTP `/rpc`** (control plane) — app-global; counts the binary-serialised protobuf message body
  of each unary request and response via a Connect `Interceptor`, read from the shared
  `TrafficMeterRegistry` under the `"http"` scope.

(Before 2026-07-24 the data plane was the focused session's LiveKit-room transport meter — a single
room-scoped `TrafficMeterRegistry` entry — which excluded backgrounded sessions and reset on every
focus switch.)

### Ping measurement

Ping uses the WebRTC peer-connection `getStats()` API (`currentRoundTripTime` from the
succeeded candidate-pair), polled every 2 seconds. The value reflects the true network
RTT to the LiveKit gateway. Displayed as `—` when the stats entry is absent or the Room
is not yet connected.

### Component hierarchy

```
SessionMainPane
 ├─ SessionTrafficStrip        ← new, flex-shrink-0 top strip
 ├─ Inspector toggle row       ← existing
 └─ terminal container
```

`useSessionLiveKitRoom(attachment)` — new hook that connects a `Room` for the selected
LiveKit session (mirrors `useCommonRoom`) and provides it to `useLiveKitPing` and the
meter's room subscription.

### Acceptance criteria

1. The strip is visible at the top of `SessionMainPane` when a session is `connected-livekit`.
2. The strip is absent when no session is selected or the session is `connected-grpc`/idle.
3. Bytes-in and bytes-out counters start at 0 and grow monotonically within a session.
4. Live rates reset toward 0 when no RPC traffic occurs for ≥ 2 s.
5. Ping shows a numeric ms value when the WebRTC candidate-pair RTT is available.
6. Ping shows `—` when RTT is null (Room not connected, stats unavailable).
7. The data-plane total is the sum of every attached runtime's terminal byte tap (focused +
   backgrounded) and is **not** reset by switching focus; the HTTP meter persists app-wide.
   A disconnected runtime stops contributing.

## Terminal Control — "Claim terminal" CTA

> **Updated: 2026-06-26** — Adds a single-screen control mutex to `SessionsDrawerScreen`.

When a session has an active terminal controller (another browser tab or device), the
`SessionMainPane` shows a **"Claim terminal"** overlay over the terminal container. The overlay
names the holding screen and provides a button to steal control.

### Overlay

- Rendered inside `SessionMainPane` when `terminalControl.isController === false`.
- Full-cover absolute scrim over the terminal container (`data-testid="terminal-control-overlay"`),
  matching the `terminal-coder-unavailable` overlay style in `GhosttyTerminalLiveKit`.
- Contains:
  - A brief message: "Controlled by another screen".
  - The holder screen identifier (`data-testid="terminal-control-holder"`).
  - A primary `<Button>` labelled **"Claim terminal"** (`data-testid="terminal-claim-btn"`).
- Clicking the button calls `onClaim()` → `ClaimTerminalControl({steal: true})`.
- When this screen holds control (`isController === true`), no overlay is rendered.
- **Disconnected sessions never show it.** The overlay is hosted by the *focused* `SessionRuntime`,
  and `SessionMainPane` suppresses the focused runtime while the inspector is docked
  (`focused={!docked && …}`). So a session shown as `disconnected` renders no
  `sessions-detail-terminal-container` and no control overlay, even if its runtime is still in the
  registry — the runtime layer stays mounted behind the docked inspector (background sessions keep
  streaming).

### Data flow

1. `SessionsDrawerScreen` owns `useTerminalControl(connectedSessionId, sessionToken)`.
2. On session attach, the hook calls `ClaimTerminalControl({steal: false})` to try to become
   the controller. If denied, `controlState.isController = false` and the CTA shows.
3. The hook then subscribes via `WatchTerminalControl` (reconnecting `for await` loop, same
   pattern as `useTaskListStream`). Each `TerminalControlEvent` is folded through
   `applyTerminalControlEvent` (pure reducer, `terminalControlState.ts`).
4. `SessionsDrawerScreen` passes `{ ...controlState, onClaim }` as the `terminalControl` prop
   to `SessionMainPane`.
5. The `control_token` from `ClaimTerminalControlResponse` is stored in the hook and forwarded
   in `SendTerminalInput` and any other control RPCs.

### Screen identity

`getScreenId()` (`src/lib/screenId.ts`) returns a stable per-tab id from `sessionStorage`,
reusing the pattern of `presenceIdentity.ts`. Two browser tabs for the same user get distinct
ids, so they do not share a lease.

### New RPCs used

- `ConnectionService.ClaimTerminalControl` — issued on session attach and on "Claim terminal" click.
- `ConnectionService.WatchTerminalControl` — live stream of lease changes.

---

## Known Limitations

- Multi-daemon host filtering (the `daemonInstanceId` grouping in `ConnectionScreen`) is
  deferred — sessions from all daemons appear together in the flat list.
- The old `ConnectionScreen` monolith is not retired by this change.
- Background Shell stdio is not durably captured; only available live via `WatchTask` while
  the task is in the in-memory registry.
- The HTTP `/rpc` meter is app-global (shared across all open sessions); only the LiveKit
  meter is strictly per-session.
- Per-session runtimes have no eviction cap (explicit-disconnect only); memory grows with the
  number of concurrently attached sessions.
