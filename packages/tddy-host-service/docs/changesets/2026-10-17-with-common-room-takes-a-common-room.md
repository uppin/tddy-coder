# 2026-10-17 — HostServiceImpl::with_common_room takes a CommonRoom

**Type:** Architecture

`with_common_room` is given a `CommonRoom` rather than a LiveKit `Room` slot, so the host service names no
LiveKit type and `livekit` leaves its manifest. Its forwards are unchanged.
