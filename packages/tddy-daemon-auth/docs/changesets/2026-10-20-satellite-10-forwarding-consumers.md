# 2026-10-20 — No LiveKit dependency

**Type:** Architecture

`#satellite` 10/13 (#632). `oauth_loopback_tunnel` and `codex_oauth_participant_metadata` moved to `tddy-daemon-livekit`; `livekit` and `tddy-livekit` left `[dependencies]` (`tddy-livekit` stays a dev-dependency). The room-token TTL is `token_provider::ROOM_TOKEN_TTL` (six hours, unchanged). `auth.rs`: 619 → 616 production lines.

Cross-package entry: [forwarding consumers off the LiveKit plugin](../../../../docs/dev/changesets/2026-10-20-satellite-10-forwarding-consumers.md).
