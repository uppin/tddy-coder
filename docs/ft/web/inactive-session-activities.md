# Inactive Session Activities — what a dormant session shows before it is resumed

**Component:** `SessionMainPane`, `SessionActivitiesPane`, `SessionInspectorDrawer`
**Route:** `#/sessions/:sessionId`
**Updated:** 2026-08-01
**Status:** Planned

## Overview

A session the operator selects may have no live agent behind it — the process exited, the daemon
restarted, or the work simply finished. Such a session is **inactive**: `SessionEntry.is_active` is
false and no `daemon-<instanceId>-<sessionId>` participant is present in the common room.

An inactive session has no terminal to show, but it does have a **recorded history**: the daemon
persists every ACP transcript frame and tool call to disk (`acp-transcript.jsonl`,
`agent-activity.jsonl`) and replays them over `ConnectionService.StreamAcpReplay` without needing
LiveKit or a live process. That recording — not an empty pane, and not the Inspector — is what the
operator wants to read when they open a dormant session, together with one control to bring it back.

An inactive session therefore presents:

- **Activities as the default main-pane view** — the read-only ACP transcript, full pane.
- **A `Resume` button in the pane's top bar** — always in the same place, for every inactive session.
- **The Inspector closed** — reachable on demand via the existing `Inspector` toggle, never opened
  for the operator.

## Behavior

### View selection

The main pane's base view is chosen by session liveness, on top of the existing workflow-view rule:

| Session | Base view |
|---|---|
| `recipe == "pr-stack"` | `PrStackScreen` (planned PRs) — regardless of liveness |
| tool session, `recipe != ""` | `WorkflowChatScreen` — regardless of liveness |
| any other session, **inactive** | **Activities view** |
| any other session, active | Terminal (mounted runtimes) |

Workflow views own their own chrome and their own content, and both remain meaningful when the
session is dormant — `PrStackScreen` renders the planned-PR stack from persisted state, and
`WorkflowChatScreen` is already a transcript. Neither is replaced. The Activities view takes over
exactly one surface: the terminal base view, which for an inactive session was the placeholder text
*"Select Resume to reconnect"*.

### Activities view

- Renders the session's recorded ACP transcript, read-only: agent text interleaved with enriched
  tool calls, each carrying its elapsed `+Ns` badge — the same `AgentChatView` the Agent Activity
  overlay uses.
- The transcript **snapshot loads eagerly** on mount. The overlay pulls it lazily because it is a
  popover the operator may never open; here it is the view itself, so there is nothing to defer.
- Streams over `StreamAcpReplay` in `SNAPSHOT_THEN_LIVE` via the session's owning-daemon client. An
  inactive session has no LiveKit room, so the stream rides the daemon-level client rather than a
  session-scoped one.
- Clicking a tool entry opens the existing tool-call detail dialog (`GetAcpToolCallDetail`).
- A session with no recorded activity renders an explicit empty state, not a blank pane.

### Resume

- A `Resume` button sits in the main pane's top bar, next to `Add agent` / `Code` / `Inspector`.
- It is rendered whenever the selected session is inactive — for **every** session type, including
  `pr-stack` and workflow-chat sessions whose base view is unchanged. Same position in every case.
- It calls the same handler as the Inspector's Details-tab Resume: `ConnectionService.ResumeSession`
  routed to the session's owning daemon.
- The Inspector's own Resume button is unchanged and stays where it is — two entry points, one
  handler.
- Once the resumed session becomes active, the pane returns to the terminal on its own: the base
  view is derived from liveness, so no view state has to be reset.

### Inspector

- The Inspector is **closed by default for every session**, active or inactive. Selecting an
  inactive session no longer opens it, and reaching `attachment.status === "idle"` for an inactive
  session no longer opens it.
- An attach **error** still opens the Inspector — that is a problem the operator has to see, and it
  is unrelated to session liveness.
- A deep link that names a tab (`?inspector=<tab>`) is still honoured on first activation, exactly
  as before.
- The Inspector renders as an **overlay drawer** for every session; it no longer docks as the full
  main pane. Docking existed because an inactive session's pane was empty behind it. It is no longer
  empty, and the drawer must not hide the Activities view the operator came to read.

### URL state

Nothing new enters the URL. The Activities view is *derived* from session liveness, which the web
already computes from common-room participants — it is not a navigable selection, so per the
URL-state contract it stays out of the address bar. The `inspector` and `full` params keep their
current meaning; the only change is that no `inspector` value is written when a session is selected.

## Interaction with the Agent Activity overlay

The top-bar Agent Activity overlay renders the same transcript. When the Activities view is the base
view, the overlay's icon is suppressed — one transcript per pane, not two. The overlay is unaffected
for active sessions and for inactive sessions showing a workflow view (`pr-stack`, workflow chat),
where it remains the only way to read the transcript.

## Edge cases

- **No recorded activity** — the Activities view renders its empty state; `Resume` is still present.
- **Transcript stream fails** — the view keeps showing whatever frames arrived; no fabricated
  fallback content.
- **Session becomes active while being read** (someone resumes it elsewhere) — liveness flips from
  the common-room participants, the base view swaps to the terminal, and `Resume` disappears.
- **Runtimes stay mounted** — a previously attached runtime for the inactive session remains
  mounted and unfocused behind the Activities view, so background sessions keep streaming and a
  later resume is instant. No claim-terminal overlay renders for it.
- **Agent died mid-elicitation** — a session carrying a stale `pending_elicitation` with
  `is_active = false` counts as **inactive**: it gets the Activities view and the Resume button like
  any other dormant session. A pending elicitation marks a session live only while it is actually
  active; nobody is waiting on the operator once the process is gone.
- **Unknown / not-found session** — unchanged; the existing not-found state still wins.

## Related

- [Agent Activity pane](agent-activity-pane.md) — the transcript data model and the overlay this view shares its rendering with
- [Session drawer](session-drawer.md) — the `#/sessions` screen and the Inspector drawer
- [URL state routing](url-state-routing.md) — what belongs in the address bar
