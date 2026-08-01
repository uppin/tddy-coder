# PRD: URL state routing — every navigable selection lives in the URL

**Date:** 2026-08-01
**Product area:** `docs/ft/web/`
**Status:** WIP
**Amends:** [app-shell.md](../app-shell.md) § Navigation menu / Default route,
[session-drawer.md](../session-drawer.md), [tasks-ui-realtime.md](../tasks-ui-realtime.md),
[daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md)

## Problem

`tddy-web` routes only the **top-level screen** through the URL. Everything the operator selects
*inside* a screen is React state that the address bar never sees:

- Clicking through sessions in the drawer leaves the URL at `#/sessions` — the reported bug.
  `SessionsDrawerScreen` writes `selectedSessionId` to component state and never touches the hash.
- `#/sessions/:id` is read **once, in a `useState` initializer** — so even when the hash does
  change (browser Back, an edited address bar, a pasted link into the open tab) the selection does
  not move. The one-shot `deepLinkActivatedRef` makes this explicit.
- The same holds for the task selection, the inspector tab, the create-session pane, the Code split
  pane, the task output channel, the worktrees project filter, and the RPC-playground method.
- The **selected host** (daemon) lives in `sessionStorage` only. It survives a reload *in the same
  tab*, but a link shared with a colleague, or pasted into a new tab, silently lands on a different
  host — and every session id in that link then refers to a session that host does not own.

Consequences: no shareable links to what you are actually looking at, Back does nothing useful (it
leaves the app), a reload loses the selection, and "look at this" between operators requires
verbal instructions.

## Goal

**Every navigable selection is represented in the URL, and the URL is the source of truth.**

Two directions, both required:

1. **Write** — a user-initiated selection updates the URL, pushing a history entry.
2. **Read** — a URL change from any source (Back/Forward, edited address bar, pasted link into the
   open tab, initial load) moves the app to that state, without a page reload.

## Non-goals

- Migrating off the hash router to `history.pushState` paths (`#/sessions/x` stays `#/sessions/x`).
  The hash keeps deep links working on a static bundle with no server rewrite rules.
- Putting **draft form input** in the URL: the create-session form fields, the Add-VNC-target form,
  the RPC playground's request JSON, the new-project dialog fields. Those are drafts, not
  destinations.
- Putting **confirmation dialogs** in the URL (delete confirm, passphrase prompt, branch-conflict
  dialog). A shared link must not re-arm a destructive confirmation.
- Standalone (non-daemon) mode, which uses `?url=&identity=&roomName=` query params on the real
  search string and is unchanged.

## URL grammar

All app state lives in the **hash**, as a path plus a hash-local query string:

```
#/<path>?<params>
```

### Paths

| Path | Screen / state |
|------|----------------|
| `#/` | Canonicalised to `#/sessions` (replace, no history entry) |
| `#/sessions` | Sessions drawer, nothing selected |
| `#/sessions/new` | Sessions drawer with the create-session pane open |
| `#/sessions/:sessionId` | That session selected (and auto-connected when active) |
| `#/sessions/:sessionId/add-agent` | Peer-spawn ("Add agent") pane for that session |
| `#/tasks` | Tasks drawer, nothing selected |
| `#/tasks/:taskId` | That task selected |
| `#/worktrees` · `#/projects` · `#/vms` · `#/livekit` · `#/rpc-playground` | As today |

`new` is reserved as a session-id segment. Session ids are UUIDs, so the reservation cannot
collide with a real session.

### Params

| Param | Applies to | Values | Meaning |
|-------|-----------|--------|---------|
| `host` | every path | daemon instance id | The selected daemon |
| `inspector` | `#/sessions/:id` | `details` `tools` `usage` `worktree` `files` `vnc` `screen-sharing` | Inspector open, on that tab. **Absent ⇒ closed.** |
| `full` | `#/sessions/:id` with `inspector` | `1` | Inspector expanded rather than docked |
| `code` | `#/sessions/:id` | `1` | Worktree Code split pane open |
| `channel` | `#/tasks/:taskId` | channel id | Selected task-output channel tab |
| `project` | `#/worktrees` | project id | Worktrees project filter |
| `participant` `service` `method` | `#/rpc-playground` | identity / name / name | Playground selection |

Unknown params are **preserved** across navigation within a screen and dropped on a screen change.
A param whose value no longer resolves (a session id that left the list, an `inspector` tab name
that is not a tab) falls back to the default state rather than erroring — except a `#/sessions/:id`
that resolves to no session, which keeps the existing "session not found" state.

Values are percent-encoded; `sessionsDrawerPathForSession` already does this for the path segment.

### Example

```
#/sessions/9f3c2b10-0000-0000-0000-000000000001?host=laptop-01&inspector=worktree&code=1
```

— session `9f3c…001` on host `laptop-01`, inspector open on the Worktree tab, Code pane open.

## History semantics

| Change | Entry |
|--------|-------|
| Screen change (hamburger menu) | **push** |
| Session select, task select | **push** |
| Open create-session / Add-agent pane | **push** |
| Inspector tab click, inspector toggle, Code toggle, channel tab | **push** |
| Host change | **push** |
| Worktrees project change, playground service/method select | **push** |
| Inspector **auto**-open (idle/error) and **auto**-close (on connect) | **replace** |
| `#/` → `#/sessions` canonicalisation | **replace** |
| Seeding `host=` when the URL carried none | **replace** |
| Dropping a stale sub-selection because the host changed | folded into the host-change push |

The split is the rule "a history entry records something the operator did." The inspector opens and
closes on its own as a session connects (`SessionsDrawerScreen`'s attachment effect); those
transitions would otherwise fill the history with entries Back cannot meaningfully undo.

## Host in the URL

Precedence for resolving the selected daemon, highest first:

1. `?host=` from the URL, **if that instance id is among the daemons in the common room**
2. `sessionStorage` (`tddy_selected_daemon`) — unchanged, still per-tab
3. `servingInstanceId` (the daemon that served the bundle)
4. The first daemon in the room
5. `null` when the room has no daemons yet

This inserts the URL above the existing chain in
[`resolveSelectedDaemonInstanceId`](../../../packages/tddy-web/src/rpc/selectedDaemon.tsx); rules
2–5 are untouched, including the "empty daemon list is *no information yet*, never clear the
selection" invariant.

Choosing a host writes **both** `?host=` (push) and `sessionStorage`, so a tab opened from a plain
`#/sessions` link still restores the last host used in that tab. When the URL carries no `host` and
resolution picks one, the resolved id is written back with **replace**, so the very first reload or
copy of the address bar already carries a host.

**Host change drops the sub-selection.** `SelectedDaemonProvider` remounts the screen subtree on a
host change (`key={selectedInstanceId}`), and a session id from the old host means nothing on the
new one. Switching from `#/sessions/abc?host=h1` navigates to `#/sessions?host=h2` — one push.

## Requirements

1. Selecting a session in the drawer navigates to `#/sessions/:id`, preserving `host`.
2. Back after selecting several sessions steps back through them, and each step re-selects that
   session (and re-attaches it when it is active).
3. A hash change from outside the app selects the named session with no page reload.
4. A `#/sessions/:id` deep link on load selects and auto-connects that session (existing behaviour,
   now driven by the same code path rather than a one-shot mount read).
5. Opening the create-session pane navigates to `#/sessions/new`; cancelling returns to
   `#/sessions`; a successful create navigates to the new session's `#/sessions/:id`.
6. "Add agent" navigates to `#/sessions/:id/add-agent`.
7. The inspector's open/expanded state and active tab round-trip through `inspector` / `full`.
8. The Code split-pane toggle round-trips through `code=1`.
9. Selecting a task navigates to `#/tasks/:taskId`; the output channel tab round-trips through
   `channel`.
10. The worktrees project filter round-trips through `project`.
11. The RPC playground's participant / service / method round-trip through their params.
12. The selected host round-trips through `host`, at the precedence above, and a host change drops
    the session sub-selection.
13. Every screen reachable from the hamburger menu keeps working exactly as today when its URL
    carries no params — the defaults are unchanged.

## Acceptance criteria

- Clicking three sessions, then Back twice, lands on the first — with that session selected in the
  drawer and its detail pane showing.
- Copying the address bar mid-session and opening it in a fresh tab reproduces the screen: same
  host, same session, same inspector tab, same Code pane state.
- Reloading `#/sessions/:id?host=h2` selects host `h2` — not the serving daemon, not the
  `sessionStorage` value.
- No behaviour change for a bare `#/sessions` (or `#/`) load.
