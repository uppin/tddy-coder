# 2026-11-10 — the inherited host-session socket adoption is deleted

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `inherited_host_sockets.rs`, `HostSessionSockets::adopt_inherited` and its `inherited` map are
  deleted: the supervisor hands a daemon only its `user.<os user>` sockets, so nothing ever produced a
  `host-session.` descriptor to adopt. Resolves the backlog entry
  `2026-11-09-dead-inherited-host-session-socket-adoption` (deleted).
- Functions brought under 60 lines: `ensure_bound` 75 → 52, `harness_launch::launch` 74 → 59,
  `delete_session_as` 94 → 52.
- Four existing `complexity-*` records gained a measurement row (each +1 line from the stack); the
  `ensure_project_available_for_start` record gained a row for one reworded comment line.
