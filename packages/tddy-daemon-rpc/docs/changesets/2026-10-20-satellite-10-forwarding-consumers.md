# 2026-10-20 — Forwards through the kernel

**Type:** Architecture

`#satellite` 10/13 (#632). Exec-tool and project forwards call `tddy_daemon_kernel::peer_calls` and `peer_forwarding`; `tddy-daemon-livekit` is a dev-dependency only. `coordinate_handlers.rs`: 660 → 652 production lines.

Cross-package entry: [forwarding consumers off the LiveKit plugin](../../../../docs/dev/changesets/2026-10-20-satellite-10-forwarding-consumers.md).
