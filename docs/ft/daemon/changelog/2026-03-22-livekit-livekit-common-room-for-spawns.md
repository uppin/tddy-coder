# 2026-03-22 — LiveKit: `livekit.common_room` for spawns

- When **`livekit.common_room`** is set (non-empty), daemon-spawned **`tddy-*`** processes receive **`--livekit-room`** set to that value so all sessions share one room; **`--livekit-identity`** remains **`daemon-{session_id}`** per session. If unset or whitespace-only, the room name is **`daemon-{session_id}`** as before.
