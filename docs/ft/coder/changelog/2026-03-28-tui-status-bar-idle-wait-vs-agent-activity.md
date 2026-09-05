# 2026-03-28 — TUI status bar: idle wait vs agent activity

- **Activity**: In `Running`, the status line uses the fast spinner and live goal elapsed. In clarification waits (`Select`, `MultiSelect`, `TextInput`), the displayed goal elapsed is frozen and the leading indicator is a one-second ·/• pulse; `VirtualTui` periodic renders align (~200 ms vs ~1 s) so streamed frames match local behavior without unnecessary traffic during waits.
- **Code**: `status_bar_activity` module; `ViewState` anchors for frozen elapsed and idle phase; shared `draw()` for local and remote.
- **Docs**: [tui-status-bar.md](../tui-status-bar.md), `packages/tddy-tui/docs/architecture.md`.
