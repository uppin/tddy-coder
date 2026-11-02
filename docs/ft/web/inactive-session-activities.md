# Inactive Session Activities — what a dormant session shows before it is resumed

**Component:** `SessionMainPane`, `DormantSessionRuntime`, `DormantAgentTerminal`, `SessionActivitiesPane`, `SessionInspectorDrawer`
**Route:** `#/sessions/:sessionId`
**Updated:** 2026-11-02
**Status:** Shipped

## Overview

A session the operator selects may have no live agent behind it — the process exited, the daemon
restarted, or the work simply finished. Such a session is **inactive**: `SessionEntry.is_active` is
false and no `daemon-<instanceId>-<sessionId>` participant is present in the common room.

An inactive session has no running agent, but it does have **recorded history**, in two forms:

- the agent terminal's output, which the daemon records and serves after the process exited
  (`GetTerminalHistory` for terminal `main`);
- every ACP transcript frame and tool call (`acp-transcript.jsonl`, `agent-activity.jsonl`), replayed
  over `activity.ActivityService.StreamAcpReplay` without needing LiveKit or a live process.

That recording — not an empty pane, and not the Inspector — is what the operator wants to read when
they open a dormant session, together with one control to bring it back.

An inactive session therefore presents:

- **The greyed Agent tab** — what the agent terminal last printed, read-only, in the terminal's
  history colour scheme, with a toast "This session is not running" and **Resume** over it. The
  strip's other tabs — shells, CLI harnesses, agent conversations — work on a stopped session too;
  see [Session terminal tabs § Stopped sessions](session-terminal-tabs.md#stopped-sessions).
- **The recorded transcript in the Inspector's Activity tab** — offered only while the session is
  stopped.
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
| any other session, **inactive** | **Tab strip** with the greyed recorded output in its Agent tab |
| any other session, active | Terminal (mounted runtimes) |

Workflow views own their own chrome and their own content, and both remain meaningful when the
session is dormant — `PrStackScreen` renders the planned-PR stack from persisted state, and
`WorkflowChatScreen` is already a transcript. Neither is replaced. The stopped strip takes over
exactly one surface: the terminal base view, where an active session shows its agent's terminal.

### The greyed Agent tab

- A read-only terminal filled with the agent terminal's whole recording (`GetTerminalHistory` for
  `main`, read forward to the end the daemon reports), drawn in the **history colour scheme** — the
  same greyed theme and greyscale filter as the terminal's "View history" — so it never reads as live.
- Keystrokes go nowhere: no `SendTerminalInput` is issued. On a touch device a tap does not raise the
  keyboard over it.
- With no recording (`NOT_FOUND`, or an empty one) it says **No recorded output**. Any other failure
  to read the recording is shown as what it is, never as an empty recording.
- A **toast** over it says "This session is not running" with a **Resume** action (see
  [Resume](#resume)).

### Inspector Activity tab

The recorded ACP transcript is the Inspector's **Activity** tab (`?inspector=activity`), which exists
only while the session is stopped.

- Renders the session's recorded ACP transcript, read-only: agent text interleaved with enriched
  tool calls, each carrying its elapsed `+Ns` badge — the same transcript surface the Agent Activity
  overlay uses.
- The transcript **loads eagerly** when the tab opens. The overlay pulls it lazily because it is a
  popover the operator may never open; here the operator asked for it.
- Streams over `StreamAcpReplay` in **`TAIL_THEN_LIVE`** via the session's owning-daemon client: the
  newest page only, opened at the newest entry, with older history paged in as the operator scrolls
  back. A long-running session no longer transfers its whole recorded history to show the end of it.
  See [Agent Activity pane § Tail-first, auto-scrolling transcript](agent-activity-pane.md). An
  inactive session has no LiveKit room, so the stream rides the daemon-level client rather than a
  session-scoped one.
- Clicking a tool entry opens the existing tool-call detail dialog (`GetAcpToolCallDetail`).
- A session with no recorded activity renders an explicit empty state, not a blank pane.
- A live session has no Activity tab: its transcript is the Agent Activity overlay's. A
  `?inspector=activity` link on a live session opens Details, and is honoured again if the session
  stops — the same rule as a media tab on a host that cannot carry tracks.
- A stopped PR-Stack or workflow session has the Activity tab too: the rule is "a stopped session",
  not "a stopped session showing the strip".

### Resume

- The greyed Agent tab's toast offers **Resume**. While it runs the action shows a spinner and cannot
  be pressed twice; a refused resume shows its reason inside the toast and the greyed view stays.
- A `Resume` button also sits in the main pane's top bar, next to `Add agent` / `Code` / `Inspector`.
- It is rendered whenever the selected session is inactive — for **every** session type, including
  `pr-stack` and workflow-chat sessions whose base view is unchanged. Same position in every case.
- The toast, the top bar and the Inspector's Details-tab Resume call one action:
  `ConnectionService.ResumeSession` routed to the session's owning daemon. A refusal is the pane's
  error for the top bar and the Inspector, and the toast's error for the toast.
- **Resume goes live at once.** A successful `ResumeSession` marks the session live in this browser
  until a session listing reports it live itself, after which the listing alone decides again. The
  pane returns to the live terminal strip immediately, not at the next 2s `ListSessions` poll. The
  base view is still derived from liveness — the mark is a fresher liveness fact, not navigation — so
  no view state has to be reset. Terminal tabs opened while the session was stopped are listed by the
  live strip, the same terminals, and open agent conversations carry over uncancelled.

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
  empty, and the drawer must not hide the stopped strip the operator came to read.
- A stopped session's Inspector offers the **Activity** tab — the recorded transcript (see
  [Inspector Activity tab](#inspector-activity-tab)).

### URL state

The stopped strip is *derived* from session liveness, which the web already computes from
common-room participants — it is not a navigable selection, so per the URL-state contract it stays
out of the address bar. The `inspector` and `full` params keep their meaning; `inspector` gains one
value, `activity`, valid only for a stopped session. No `inspector` value is written when a session
is selected.

## Interaction with the Agent Activity overlay

The top-bar Agent Activity overlay renders the same transcript. While a stopped session shows its
strip, the overlay's icon is suppressed — one transcript per pane, not two: the Inspector's Activity
tab is that session's transcript route. The overlay is unaffected for active sessions and for
inactive sessions showing a workflow view (`pr-stack`, workflow chat).

## Edge cases

- **No recorded output** — the greyed Agent tab says "No recorded output"; the toast and `Resume` are
  still present.
- **No recorded activity** — the Activity tab renders its empty state; `Resume` is still present.
- **Transcript stream fails** — the Activity tab keeps showing whatever frames arrived; no fabricated
  fallback content.
- **Resume refused** — the toast shows the daemon's reason and the greyed view stays; nothing is
  marked live.
- **Session becomes active while being read** (someone resumes it elsewhere) — liveness flips from
  the common-room participants, the base view swaps to the terminal, and `Resume` disappears.
- **Runtimes stay mounted** — a previously attached runtime for the inactive session remains
  mounted and unfocused behind the stopped session's strip, so background sessions keep streaming and a
  later resume is instant. No claim-terminal overlay renders for it.
- **Agent died mid-elicitation** — a session carrying a stale `pending_elicitation` with
  `is_active = false` counts as **inactive**: it gets the stopped strip and the Resume button like
  any other dormant session. A pending elicitation marks a session live only while it is actually
  active; nobody is waiting on the operator once the process is gone.
- **Unknown / not-found session** — unchanged; the existing not-found state still wins.

## Related

- [Agent Activity pane](agent-activity-pane.md) — the transcript data model and the overlay this view shares its rendering with
- [Session drawer](session-drawer.md) — the `#/sessions` screen and the Inspector drawer
- [URL state routing](url-state-routing.md) — what belongs in the address bar
