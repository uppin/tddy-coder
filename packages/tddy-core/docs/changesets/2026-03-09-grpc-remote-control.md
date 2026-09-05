# 2026-03-09 — gRPC Remote Control

**Type:** Feature

PresenterEvent enum and PresenterHandle (broadcast::Sender + mpsc::Sender). Presenter gains optional broadcast_tx; with_broadcast() builder. After each view callback in poll_workflow() and handle_intent(), events broadcast to subscribers. tokio sync feature for broadcast. (tddy-core)
