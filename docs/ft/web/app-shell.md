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

## Default route

The route switch's catch-all now renders `SessionsDrawerScreen`, so **`#/` opens the
sessions drawer**. The legacy `ConnectionScreen` and its `#/terminal/:id` route are removed;
deep-linking to a session uses `#/sessions/:id`. A `#/sessions/<unknown-id>` that resolves to
no known session (after the list loads) shows a "session not found" state with a Home link
(`terminal-route-unknown-session` / `-home`).

## LiveKit screen

**Route:** `#/livekit` · **Component:** `LiveKitAppPage`
(`packages/tddy-web/src/components/livekit/`)

Renders the "Connected participants" panel (`data-testid="connected-participants-panel"`)
extracted from the old `ConnectionScreen`: the pure `ParticipantList` fed by
`useRoomParticipants(room)` and `useObservedCommonRoomStatus(room)`, where `room` is the
shared common room from `useSelectedDaemon()`.

## Theme

Every screen uses the shared shadcn theme (`src/index.css` tokens, `.dark` navy palette).
The standalone `ConnectionForm` and `DaemonLoginScreen` in `src/index.tsx` use theme-token
classes (`text-foreground`, `text-muted-foreground`, `text-destructive`, `border-input`,
`bg-background`) instead of inline hex, matching the rest of the app.
