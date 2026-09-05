# 2026-07-23 — Inspector docks as the main pane for disconnected sessions

- The Session Inspector now **docks as the full main pane** for a `disconnected` session instead of a ~360px right-edge overlay drawer; `connected` / `needs-input` sessions keep the drawer. Driven by a `data-docked` attribute (from the pure `isInspectorDocked(session)` helper); all header controls remain in both layouts. See [session-drawer.md](../session-drawer.md#docked-vs-drawer).
- As a consequence, the **"Claim terminal"** overlay is no longer shown for a `disconnected` session: `SessionMainPane` suppresses the focused runtime while docked, so the runtime foreground (and its control overlay) is not rendered, while the runtime layer stays mounted behind the inspector (background sessions keep streaming).
