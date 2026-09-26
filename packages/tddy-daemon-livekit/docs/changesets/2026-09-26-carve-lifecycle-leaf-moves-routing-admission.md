# 2026-09-26 — Holds peer routing and room admission

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`peer_routing` (`PeerRouting`) and `session_admission_service` moved here from
`tddy-session-lifecycle`, which re-exports both by name. No new dependencies. Four `PeerRouting`
methods (`set_eligible_daemon_source`, `eligible_instance_ids`, `classify_daemon_route`,
`stream_served_by_peer`) are `pub`, because lifecycle's host delegates to them. Production lines
5,409 → 5,863; tests 174 passed, unchanged (neither module had tests). See
[livekit-service.md](../livekit-service.md#where-the-code-lives). The six code issues were not touched.
