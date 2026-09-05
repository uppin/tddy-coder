# 2026-04-11 — Embedded tddy-daemon

**Type:** Feature

Main process spawns `tddy-daemon` when `TDDY_DAEMON_CONFIG` is set; binary from `TDDY_DAEMON_BINARY`, `resources/bin/tddy-daemon` (prebuild), or workspace `target/{release,debug}`; cleanup on exit; `build.copy` + `scripts/build-daemon-for-desktop.sh`. Feature **[tddy-desktop-electrobun.md](../../../../docs/ft/desktop/tddy-desktop-electrobun.md)** (bundled daemon section). (tddy-desktop)
