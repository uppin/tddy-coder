# 2026-07-23 — Unified app shell layout

- All daemon-mode screens now render inside a single **`AppShell`** that owns the top chrome (top-left hamburger `DaemonNavMenu` + title + daemon selector + user avatar) with `scroll` and `fullbleed` variants. Screens no longer hand-roll their own header, so none can ship without the navigation menu — the sessions drawer screen previously did. See [app-shell.md](../app-shell.md).
- The **sessions drawer** (`#/sessions`) is now the **default** route: `#/` resolves to it and the legacy `ConnectionScreen` (plus its `#/terminal/:id` route) is removed. A `#/sessions/<unknown-id>` deep link shows a "session not found" state with a Home link; bulk select + delete of sessions is available in the drawer.
- The LiveKit "Connected participants" table moves out of the old connection screen into its own **LiveKit** screen (`#/livekit`, new hamburger item), reusing the shared common-room participant hooks.
- The standalone auth/connection forms use shared shadcn theme tokens instead of inline hardcoded hex, so every screen shares one theme.
