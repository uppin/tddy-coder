# tddy-session-web

Sessions: the seams session screens reach the app through (`SessionShellProvider` /
`SessionShell`), the session helpers (indicator and notification registry, participant metadata,
labels, grouping, sorting, selection, connection status, traffic meter, transport flavour, LiveKit
ping, the notification feed), the session runtime with its terminals, the main pane, inspector,
agent layers and code pane, and the sessions drawer with project home, the status bar and the traffic
strip — every session surface the app shows. It composes the terminal, chat, hosts, workspace, PR-stack
and create-session packages.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

The drawer's dot for a notification is `lib/sessionNotificationDot.ts`'s `dotForNotification`:
attention lights yellow and activity blinks green, except a session's end (source `SESSION_ENDED`,
`#activity-wake` 1/3), which lights nothing — an ended session already reads as stopped.

A drawer row closes its session (`SessionDrawerCloseControl.tsx`: the × on an active row, the trash
bin on an inactive one that still holds a worktree, a spinner while its close runs). The click
previews the close and runs it at once when nothing needs confirming; otherwise `CloseSessionDialog.tsx`
shows each worktree's losses, an unreadable worktree's reason with Retry, and the children choice.
`useSessionCloses.ts` tracks closes per session, concurrently, routed to the owning host; failed steps
land in `SessionsDrawerNotices`. Product contract: `docs/ft/web/session-drawer.md` § Closing a session.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients` and port doubles).

Documentation: [docs/README.md](docs/README.md).
