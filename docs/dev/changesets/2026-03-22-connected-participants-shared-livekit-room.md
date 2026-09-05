# 2026-03-22 — Connected participants + shared LiveKit room

**Type:** Feature

`livekit.common_room` in daemon YAML → `/api/config`; `tddy-web` `useCommonRoom` / `useRoomParticipants` / `ParticipantList` on daemon **ConnectionScreen**; spawner uses common room for **`--livekit-room`** when set; **`App`** renders **ConnectionScreen** vs **ConnectionForm** without mixing element and component types. Feature docs: `docs/ft/web/web-terminal.md`, `docs/ft/web/changelog.md`, `docs/ft/daemon/changelog.md`. (tddy-daemon, tddy-coder, tddy-web)
