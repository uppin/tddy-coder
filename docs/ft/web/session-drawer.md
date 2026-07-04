# Session Drawer Screen

**Route:** `#/sessions`  
**Component:** `SessionsDrawerScreen` (`packages/tddy-web/src/components/sessions/`)

## Overview

A focused session management screen with a left-side drawer listing all sessions and a
main content area on the right. A Session Inspector can be opened on the right edge to
show session details and controls; it is hidden by default for connected sessions and
open by default for disconnected sessions.

The screen is a parallel view to the existing `ConnectionScreen` (`#/`). Both routes remain
available; no existing behaviour is changed.

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

Sessions are ordered newest-first by `createdAt` (`sortSessionsByCreation()`), with no
active-first grouping (contrast with `sortSessionsForDisplay` used by `ConnectionScreen`).

## Connection Status Token

`connectionStatusForSession(entry)` maps proto fields to one of:
- `"connected"` — `isActive: true` and `pendingElicitation: false`
- `"needs-input"` — `pendingElicitation: true` (takes precedence over `isActive`)
- `"disconnected"` — `isActive: false` and `pendingElicitation: false`

## Session Attachment

`useSessionAttachment` hook manages the single-session attach lifecycle:
- `connectSession` → calls `ConnectSession` RPC → `connected-livekit` or `connected-grpc`
- `resumeSession` → calls `ResumeSession` RPC → same state transitions
- Clicking a connected session in the drawer auto-calls `connectSession`
- Clicking a disconnected session opens the inspector by default without auto-connecting

## Session Inspector Drawer

The Session Inspector is an overlay panel anchored to the right edge of `SessionMainPane`.
It shows session metadata and action controls. Its visibility is controlled by a
`data-state` attribute with three values.

### Open/Expand State

- `data-state="closed"` — hidden (default for connected sessions)
- `data-state="open"` — overlay panel ~360px wide, floats over the terminal with a slight
  scrim; does NOT resize/push the terminal
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

- `ListSessions` — session list with `isActive`, `createdAt`, `repoPath`, `workflowGoal`, `pendingElicitation`
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
| Model | claude-cli | yes — dropdown of `CLAUDE_CLI_MODELS` |
| Permission mode | claude-cli | no — auto/default/acceptEdits/plan/bypassPermissions |
| Initial prompt | claude-cli | no — textarea |
| Branch mode | both | — New branch from base (optional name + base ref) or Work on existing branch (select from `ListProjectBranches`) |

The tool binary (`toolPath`) is auto-selected from `ListTools`; shown as a select only
when multiple tools are available.

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

### Post-Create

On success, `SessionsDrawerScreen` navigates to `/sessions/:newId` and auto-attaches
(same behaviour as clicking an active session in the drawer).

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
- `ListSessions` — populate PR stack parent picker (best-effort; failure hides the picker)
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

## Per-Workflow Session Views

> **Added: 2026-07-01** — a session's `recipe` can now select a fully custom main-pane
> screen instead of the terminal. First (and currently only) consumer: the PR-Stack Chat
> Screen below.

### View registry

`SessionMainPane.tsx` gains a resolution step before the existing
`isConnected ? <terminal> : <placeholder>` branch:

```typescript
const customView = resolveWorkflowView(selectedSession);
if (customView) return customView;
```

`resolveWorkflowView(session)` (`src/components/sessions/workflowViews.tsx`) is a small
registry keyed by `session.recipe`:

```typescript
resolveWorkflowView({ recipe: "pr-stack", ... }) → <PrStackScreen session={...} />
resolveWorkflowView({ recipe: "tdd", ... })       → null   // falls through to the terminal
```

Custom views own their own connection/chrome and render **in place of** the terminal
container — they are not gated on `attachment.status`; a `pr-stack` session shows its
screen whether or not a terminal is attached, since the whole point is that the operator
never needs the raw terminal for this workflow. Every other recipe is unaffected: the
existing terminal / placeholder behaviour is the fallback when no custom view is
registered.

### Data source

`SessionEntry.recipe` (proto field 22, already surfaced for the recipe dropdown and stack
grouping above) is the sole routing key — no new field was needed to decide *which* view
opens.

## PR-Stack Chat Screen

The custom screen for `recipe === "pr-stack"` sessions. Replaces the terminal with two
panes: a live list of the planned PRs (left) and a chat window backed by a remote
Presenter (right). Lets the operator review a freshly-written stack plan, keep refining it
by chatting with the agent, and start child sessions for each planned PR — all without
leaving the orchestrator session.

**Component:** `PrStackScreen` (`src/components/sessions/prstack/PrStackScreen.tsx`), with
`PlannedPrList` / `PlannedPrRow` subcomponents.

### Planned-PR list

Reads the orchestrator's `Stack` (see [PR stacking § Stack data model](../coder/pr-stacking.md#stack-data-model))
via `SessionEntry.stackPlanJson` (proto field 23 — a JSON-serialized `Stack`, empty string
until a plan exists) and renders one row per `StackNode` in `Stack::topo_order` (roots
before dependents):

- **Unspawned node** (`session_id` empty) — shows a **"Start session"** CTA
  (`data-testid="pr-stack-start-session-<nodeId>"`).
- **Spawned node** (`session_id` set) — shows a status chip
  (`data-testid="pr-stack-status-chip-<nodeId>"`) derived from `pr_status.phase` /
  `child_state` instead of the CTA.

### Start session CTA

Clicking "Start session" on an unspawned node opens the shared **`CreateSessionDialog`** — the
same `CreateSessionPane` form the sessions drawer uses — **pre-filled** from the node and its
orchestrator, so the operator can review and adjust before spawning:

- `projectId` and host (`daemonInstanceId`) come from the orchestrator session.
- `stackParent` = this orchestrator session's id; `sessionType` = `"claude-cli"`.
- Branch mode = `new_branch_from_base`, `newBranchName` = the node's `branch ?? branchSuggestion`.
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

The right-hand chat panel is a thin UI over the session's existing **remote Presenter**
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

**Component:** `PrStackChat` (`src/components/sessions/prstack/PrStackChat.tsx`) +
`usePresenterChat` hook.

### New RPCs / proto fields used

- `SessionEntry.stack_plan_json` (proto field 23, new) — JSON-serialized `Stack` for the
  planned-PR list.
- `TddyRemote.Stream` (existing) — chat window transport.
- `ConnectionService.StartSession` (existing, `recipe` + `stack_parent`) — the Start session CTA.

## Session Traffic Strip

A thin `flex-shrink-0` strip rendered at the top of `SessionMainPane` whenever a session
is in `connected-livekit` state. It provides live visibility into RPC throughput and
connection health for the selected session.

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

Two transport layers are metered independently and summed for display:

- **LiveKit data-channel** — per-session; counts exact wire payload bytes at the point
  they are serialised/deserialised (outbound `publishRequest` payload, inbound
  `DataReceived` payload).
- **HTTP `/rpc`** — app-global; counts the binary-serialised protobuf message body of
  each unary request and response via a Connect `Interceptor`.

Both meters share a `TrafficMeterRegistry` (React context) keyed by scope:
`"http"` for the HTTP transport and the LiveKit room name for the data-channel transport.

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
7. Switching sessions resets the session-scoped (LiveKit) meter to 0; the HTTP meter persists.

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

- The terminal in the main pane is a placeholder; real terminal mounting is out of scope.
- Multi-daemon host filtering (the `daemonInstanceId` grouping in `ConnectionScreen`) is
  deferred — sessions from all daemons appear together in the flat list.
- The old `ConnectionScreen` monolith is not retired by this change.
- Background Shell stdio is not durably captured; only available live via `WatchTask` while
  the task is in the in-memory registry.
- The HTTP `/rpc` meter is app-global (shared across all open sessions); only the LiveKit
  meter is strictly per-session.
