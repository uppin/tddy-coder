# 2026-11-27 — ConnectTarget signature and wheel buttons (PR #1060)

**Type:** Feature

Adopts `ScreenSharingClient::connect(&ConnectTarget, …)` / `inspect(&ConnectTarget)`. `inject_wheel`
clicks RFB buttons 4–7 (up, down, left, right) once per axis that moved (`wheel_button_masks`,
`tests/wheel_buttons.rs`).
