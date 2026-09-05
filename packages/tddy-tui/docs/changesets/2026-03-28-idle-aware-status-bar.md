# 2026-03-28 — Idle-aware status bar

**Type:** Feature

Agent-active (`Running`) keeps fast spinner and live goal elapsed; clarification waits (`Select`/`MultiSelect`/`TextInput`) freeze displayed elapsed and use ·/• pulse at 1 Hz; `status_bar_activity` module; `ViewState` freeze anchors; `virtual_tui_periodic_render_interval`; spinner_tick gated to agent-active. Feature doc `docs/ft/coder/tui-status-bar.md`, architecture updated. (tddy-tui)
