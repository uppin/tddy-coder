# 2026-10-24 — SpawnRelayed: a coding-agent harness with its stdio relayed to the caller
**Type:** Feature

New request `SpawnRelayed`, gated by its own allowlist `spawn_policy.allowed_relay_tool_paths` (empty
by default) beside the session-user and environment gates; the supervisor connects to the caller's
socket only if it is a non-symlink socket owned by the caller's uid whose listener's `SO_PEERCRED`
uid matches, before forking. `ForkBroker` forks with piped stdio for it; a caller hang-up stops the
child. See [architecture.md § Relayed spawns](../architecture.md#relayed-spawns-the-one-child-with-stdio)
(#721, `#coagents` 2/8).

Restructured (engine moves, production lines): `server.rs` 708 → 322 (+ `server/listener.rs`,
`server/rpc.rs`), `supervisor.rs` 965 → 370 (+ `supervisor/session_table.rs`, `service_slot.rs`,
`service_restart.rs`), `config.rs` 498 → 456 (+ `config/spawn_policy.rs`), `spawn_broker.rs`
1,526 → 944 (+ `spawn_broker/fork_broker.rs`, `child_environment.rs`, `accounts.rs`; the rest is
recorded in `oversized-file-spawn-broker`). Code issues `complexity-server-serve` →
`complexity-listener-serve` and `complexity-supervisor-reclaim-scope` →
`complexity-session-table-reclaim-scope` moved with their functions, unchanged (95 and 49 lines).
