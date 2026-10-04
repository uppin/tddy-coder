# 2026-10-04 — Forwarding SetProjectAccounts to peers

**Type:** Feature

`forward_set_project_accounts_via_livekit` in `livekit_peer_discovery` forwards `SetProjectAccounts`
to the peer that owns the project, as its siblings do for the other project RPCs. The file grew from
1,636 to 1,658 lines; its split stays deferred. See [livekit-service.md](../livekit-service.md).
