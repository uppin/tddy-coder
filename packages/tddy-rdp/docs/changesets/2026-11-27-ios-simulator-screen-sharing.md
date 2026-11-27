# 2026-11-27 — ConnectTarget signature and wheel rotation (PR #1060)

**Type:** Feature

Adopts `ScreenSharingClient::connect(&ConnectTarget, …)` / `inspect(&ConnectTarget)`. `inject_wheel`
sends one fast-path mouse PDU per axis: `VERTICAL_WHEEL` −120 × sign(dy), `HORIZONTAL_WHEEL`
120 × sign(dx) (`wheel_mouse_pdus`, `tests/wheel_pdus.rs`).
