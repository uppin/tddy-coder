# 2026-09-10 — Session lifecycle extracted

**Type:** Architecture

`session.SessionService` (8 RPCs) and the session modules that backed family C moved from `tddy-daemon`
into this crate. Owns `TaskRegistry`. See [session-service.md](../session-service.md).
