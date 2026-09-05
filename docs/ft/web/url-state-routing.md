# URL State Routing — every navigable selection lives in the URL

**Applies to:** every daemon-mode screen
**Modules:** `packages/tddy-web/src/routing/{appLocation,useAppLocation,appRoutes,selectedHost}.ts`
**Added:** 2026-08-01

## Overview

tddy-web's address bar names what you are looking at, and changing the address bar changes what you
see. Both directions hold:

- **Write** — a selection you make (a session, a task, an inspector tab, a host) updates the URL and
  pushes a history entry.
- **Read** — a URL change from any source (Back, Forward, an edited address bar, a link pasted into
  the open tab, a reload) moves the app to that state, with no page load.

Selection is therefore *derived from* the URL rather than held beside it. There is one code path for
"show session X", and a drawer click, a deep link, and the Back button all take it.

## Why

Before this, only the **top-level screen** was routed. Everything selected *inside* a screen was
React state the address bar never saw:

- Clicking through sessions left the URL at `#/sessions` — nothing to share, nothing to return to.
- `#/sessions/:id` was read **once, in a `useState` initializer**, so even a hash that did change
  (Back, an edited address bar) did not move the selection.
- The selected host lived in `sessionStorage` only. It survived a reload *in the same tab*, but a
  link sent to a colleague — or opened in a new tab — landed on a different host, where every
  session id in that link named nothing.

## URL grammar

All app state lives in the **hash**, as a path plus a hash-local query string: `#/<path>?<params>`.
The hash (rather than real paths) keeps deep links working on a static bundle with no server rewrite
rules.

### Paths

| Path | Screen / state |
|------|----------------|
| `#/` | Canonicalised to `#/sessions` (replace — no history entry) |
| `#/sessions` | Sessions drawer, nothing selected |
| `#/sessions/new` | Sessions drawer with the create-session pane open |
| `#/sessions/:sessionId` | That session selected (and auto-attached when active) |
| `#/tasks` | Tasks drawer, nothing selected |
| `#/tasks/:taskId` | That task selected |
| `#/worktrees` · `#/projects` · `#/vms` · `#/livekit` · `#/rpc-playground` | As before |

`new` is reserved as a session-id segment. Session ids are UUIDs, so the reservation cannot collide
with a real session.

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

A **screen change** (a different first path segment) drops every screen-scoped param and keeps only
`host`. A move *within* a screen (`/sessions/abc` → `/sessions/def`) keeps them, so the inspector
does not close because you clicked the next row in the drawer.

A param that no longer resolves degrades rather than breaking: an unknown `inspector` tab falls back
to Details, an unknown `channel` to the task's first channel, an unregistered `project` to the first
listed one, an unresolvable `method` to no selection. Where the app resolves a default this way it
**writes the answer back**, so the address bar always names what is actually shown.

A param can also name a tab that exists but is **not offered on this host**. `inspector=vnc` and
`inspector=screen-sharing` are video, and a host reached over a wire that carries no tracks has
neither tab in its strip; such a link degrades to Details for exactly as long as that is true, and is
honoured the moment the wire can serve it. Degrading rather than 404-ing is the same choice the
`#/livekit` route makes — a shared link should land somewhere. See
[capability gating](../../../packages/tddy-web/docs/capability-gating.md).

Example:

```
#/sessions/9f3c2b10-0000-0000-0000-000000000001?host=laptop-01&inspector=worktree&code=1
```

— session `9f3c…001` on host `laptop-01`, inspector open on the Worktree tab, Code pane open.

## History semantics

A history entry records **something the operator did**.

| Change | Entry |
|--------|-------|
| Screen change, session select, task select | push |
| Open create-session pane | push |
| Inspector tab click, inspector toggle, Code toggle, channel tab | push |
| Host change; worktrees project change; playground service/method select | push |
| Inspector **auto**-open (idle/error) and **auto**-close (on connect) | replace |
| `#/` → `#/sessions` canonicalisation | replace |
| Writing back a resolved default (`host`, `project`, `participant`) | replace |
| Clearing session-pane params once no session is selected | replace |

The inspector opens and closes on its own as a session connects. Recording those as history entries
would fill Back with states nobody chose, so they rewrite the current entry instead.

## Host in the URL

Precedence for the selected daemon, highest first:

1. `?host=` from the URL, **if that instance id is among the daemons in the common room**
2. `sessionStorage` (`tddy_selected_daemon`) — still per-tab
3. `servingInstanceId` (the daemon that served the bundle)
4. The first daemon in the room
5. `null` when the room has no daemons yet

The URL leads because it is the only source a shared link carries. Rules 2–5 are unchanged, including
the invariant that an **empty daemon list means "no information yet", never "no daemons exist"** — the
common room is briefly empty on connect and on a reconnect, and clearing the selection then would
flash every screen to "nothing selected".

Choosing a host writes **both** `?host=` (push) and `sessionStorage`, so a tab opened from a plain
`#/sessions` link still restores the last host used in that tab. When the URL carries no `host`, the
resolved id is written back with **replace**.

**A host change drops the sub-selection.** `SelectedDaemonProvider` remounts the screen subtree on a
host change, and a session id from the old host means nothing on the new one — switching from
`#/sessions/abc?host=h1` lands on `#/sessions?host=h2`.

## Deliberately not in the URL

Draft form input (create-session fields, add-target forms, the playground's request JSON),
confirmation dialogs (delete confirm, VNC passphrase, branch conflict — a shared link must not
re-arm a destructive confirmation), the drawer's open/closed flag (a responsive-layout concern that
defaults by viewport), and bulk-selection ticks.

Standalone (non-daemon) mode is unaffected: it uses `?url=&identity=&roomName=` on the real search
string, not the hash.

## Implementation

| Module | Role |
|--------|------|
| `routing/appLocation.ts` | Pure model — `AppLocation { path, params }`, parse/format, `withParams`, `withPath`, `screenRootOf`, and the param-name constants. The only place the URL's spelling is known. |
| `routing/useAppLocation.ts` | A **module-level store** over `window.location.hash`, read via `useSyncExternalStore`. Module-level, not a React context, so a nested pane — and a component test that mounts one screen bare — shares the same source of truth without prop-drilling. Push assigns `location.hash`; replace uses `history.replaceState`; both notify synchronously. |
| `routing/appRoutes.ts` | Path grammar: the route constants, `parse*`/`*Path` helpers, and `isInspectorTabName`. |
| `routing/selectedHost.ts` | `resolveSelectedDaemonInstanceId` (the precedence above) plus the `sessionStorage` accessors, pure so they are unit-testable. |

`onNavigate` props remain the screen-change seam the shell uses; they are thin wrappers over the
store.
