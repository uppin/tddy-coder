# App Shell — Unified Layout

**Component:** `AppShell` (`packages/tddy-web/src/components/shell/AppShell.tsx`)
**Applies to:** every daemon-mode routed screen

## Overview

All daemon-mode screens render inside a single `AppShell` that owns the top chrome:
a top-left hamburger navigation menu (`DaemonNavMenu`), the screen title, the daemon/host
selector (`DaemonSelectorConnected`), and the user avatar. Screens supply only their body
content. This replaces the previous arrangement where each screen hand-rolled its own header
row, which had let the sessions screen ship without a hamburger menu.

## Why

Before this change, `tddy-web` had no shared shell: routing was a hash switch in
`src/index.tsx` and every screen built its own header. Consequences:

- The sessions drawer screen (`#/sessions`) had **no hamburger menu**, stranding users
  with no way to reach other screens.
- Two "Sessions" entries coexisted — the legacy monolithic `ConnectionScreen` (`#/`) and
  the newer drawer screen — with the legacy one still the default.
- The LiveKit "Connected participants" table was buried inside `ConnectionScreen`.
- The standalone auth/connection forms used inline hardcoded hex colors instead of the
  shared shadcn theme tokens.

## AppShell

`AppShell` accepts:

| Prop | Purpose |
|------|---------|
| `title` | Screen title shown in the header. |
| `onNavigate` | `(path) => void` — wired to the hamburger menu. |
| `variant` | `"scroll"` (default) for content screens; `"fullbleed"` for drawer screens. |
| `headerRight` | Optional extra controls placed left of the selector. |
| `dataTestId` | Forwarded to the shell root (e.g. `sessions-drawer-screen`). |
| `children` | Screen body. |

- **`scroll`** — a padded, vertically scrolling content column (the previous
  `screenShellClassName`). Used by Projects, VMs, Worktrees, RPC Playground, LiveKit.
- **`fullbleed`** — a full-height (`h-[100dvh]`) flex column with a thin header bar and a
  `flex-1 min-h-0 overflow-hidden` body so a drawer's two-pane layout and its pinned footer
  (e.g. `HostStatsFooter`) both survive. Used by the Sessions and Tasks drawer screens.

The header (hamburger + title + selector + avatar) is defined once, in `AppShell`, so no
screen can render without the navigation menu.

## Navigation menu

`DaemonNavMenu` (top-left hamburger, `data-testid="shell-menu-button"`) lists, in order:

- **Sessions** → `#/sessions` (`shell-menu-sessions`)
- **Worktrees** → `#/worktrees`
- **Tasks** → `#/tasks`
- **Projects** → `#/projects`
- **VMs** → `#/vms`
- **LiveKit** → `#/livekit` (`shell-menu-livekit`)
- **RPC Playground** → `#/rpc-playground`

The former separate "Sessions" (`#/`) and "Sessions (new)" (`#/sessions`) items are
collapsed into the single **Sessions** entry.

**The LiveKit entry is offered only where it leads somewhere.** The whole screen is participant
presence, so on a selected host reached over a wire that carries none, the entry is **removed from
the menu** rather than shown disabled — a menu item that cannot be used is worse than one that is
not there. The entry and the screen read the same rule, so they cannot disagree about what the
operator is being told: while the common room is still being joined, or has failed to join, the
entry stays, because the screen it points at is where the reason for a failed join is reported. See
[capability gating](../../../packages/tddy-web/docs/capability-gating.md).

## Default route

The route switch's catch-all renders `SessionsDrawerScreen`, so **`#/` opens the sessions drawer**;
since 2026-08-01 it is also *canonicalised* to `#/sessions` (a replace, so it costs no history
entry). The legacy `ConnectionScreen` and its `#/terminal/:id` route are removed; deep-linking to a
session uses `#/sessions/:id`. A `#/sessions/<unknown-id>` that resolves to no known session (after
the list loads) shows a "session not found" state with a Home link
(`terminal-route-unknown-session` / `-home`).

## Routing (updated 2026-08-01)

The hash switch described here now covers only the **screen**. Every navigable selection *inside* a
screen — the selected session and task, the create-session pane, the inspector's
tab and open/expanded state, the Code pane, the task output channel, the worktrees project filter,
the RPC-playground selection, and **the selected host** — is likewise carried in the URL, and the
URL is the source of truth for all of it (Back, Forward, an edited address bar and a pasted link all
move the app). `AppShell`'s `onNavigate` is now a thin wrapper over the shared location store.

See **[url-state-routing.md](url-state-routing.md)** for the full grammar, the push/replace rules,
and host precedence.

## LiveKit screen

**Route:** `#/livekit` · **Component:** `LiveKitAppPage`
(`packages/tddy-web/src/components/livekit/`)

Renders the "Connected participants" panel (`data-testid="connected-participants-panel"`)
extracted from the old `ConnectionScreen`: the pure `ParticipantList` fed by the common room's
participants and observed status, with the [rooms panel](livekit-rooms-panel.md) below it.

The route stays reachable on a host that carries no presence — a deep link should not 404 — but the
screen renders a single explanation in place of both panels, naming the connection as the reason
there is no roster and no room list. The panels themselves are not rendered empty or disabled.

The roster reports the room's real state rather than the gate's verdict. "Connecting to presence
room…" means a join is in flight; a failed join shows the reason the join failed, which is what the
operator came here for; "not available on this connection" is said only when nothing is being joined
*and* the wire carries no presence. The camera column beside each participant is a video track, so it
appears only where the connection also carries media.

## Theme

Every screen uses the shared shadcn theme (`src/index.css` tokens, `.dark` navy palette).
The standalone `ConnectionForm` and `DaemonLoginScreen` in `src/index.tsx` use theme-token
classes (`text-foreground`, `text-muted-foreground`, `text-destructive`, `border-input`,
`bg-background`) instead of inline hex, matching the rest of the app.
