# 2026-09-09 — The session room stamps ticks from 1, and serves five coordinates

`session_room.rs` kept `next_delta_seq` as a bare counter starting at 0. It now keeps
`last_delta_seq: Option<u64>` and stamps through `tddy_service::session_activity::next_tick`, so a
session's first delta is `FIRST_TICK` (1) and `NO_TICK` (0) means only what it says. `next_tick`
saturates rather than wrapping — a wrap would land the next tick back on `NO_TICK` and reintroduce
the ambiguity the change removes.

The room's `MultiRpcService` now carries **five** entries — `session_files`, `session_agents`,
`activity`, `terminal_session` and `connection` — all `Arc`s of the one `ConnectionServiceImpl`, so
the `Arc`-cycle note in [session-room.md](../session-room.md) is about five clones rather than one.
Nothing about the cycle or the `close` contract changed.

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
