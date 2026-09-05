# 2026-08-14 — LiveKit rooms and participants panel

- **A second panel on `#/livekit` lists every room on the LiveKit server and who is joined to each.** Until now the dashboard could only see the one room the browser itself joined, so per-session terminal rooms, presenter rooms and screen-share rooms were invisible — "is anybody actually in that room?" meant shelling into the host. See [livekit-rooms-panel.md](../livekit-rooms-panel.md).
- **Participant metadata is revealed on hover or keyboard focus**, pretty-printed, instead of taking a column. The row is focusable, so the metadata is reachable without a pointer.
- **Fed by a new `ConnectionService.StreamLiveKitRooms`** whose first message is a full snapshot and whose every later message is one change event, so steady-state traffic tracks churn rather than roster size — an idle server produces an idle stream.
- **The existing Connected participants panel is unchanged.** The common room appears in both; they answer different questions and are sourced independently (client SDK vs. the LiveKit server API).
- **Rooms can carry a human `label`** in their own metadata, shown beside the opaque room name. No publisher writes a `label` today — a session room's metadata is a worktree snapshot — so this plumbs the channel rather than lighting it up.
