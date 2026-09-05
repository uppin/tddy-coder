# 2026-03-14 — Per-Connection Virtual TUI

**Type:** Feature

ViewConnection, connect_view(), NoopView. Presenter decoupled from single view; views subscribe via connect_view for state snapshot + event_rx + intent_tx. (tddy-core)
