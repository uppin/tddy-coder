# 2026-11-26 — The session lifecycle crates do disk I/O through the Fs port

**Type:** Architecture · **PR:** [#1048](https://github.com/uppin/tddy-coder/pull/1048) (`#fs-port` 4/13)
**Packages:** `tddy-session-lifecycle`, `tddy-session-split`, `tddy-session-actions`, `tddy-session-agents`, `tddy-session-catalog`, `tddy-session-driver`

The production disk I/O of the six session lifecycle crates goes through the `Fs` port: each crate's
disk-touching entry points take `Arc<dyn Fs>` (or a struct holding one), with `DiskFs` at composition
sites. Every crate carries `tests/fs_port.rs` — the `direct_disk_io_in` guard over its own `src/`
plus one `MemFs` behaviour test naming the behaviour (directory listing, hook registration,
channel-manifest byte parity, session-dir records, registry keys, session-dir creation).

`session_action_pipeline.rs` (435 → 514, the one file this PR pushed past the 500-production-line
budget) is decomposed inside this PR (`spawn.rs` takes the spawners unchanged). Two files this PR
grew while already over budget (`session_agent_clone.rs`, `svc_spawn_split_agent.rs`) are recorded
and deferred to the sweep node, which meets neither this PR's diff nor any other open node's.
Detail: [the filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).
