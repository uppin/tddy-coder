# 2026-10-24 — The spawn worker serves requests concurrently

**Type:** Feature

The forked spawn worker multiplexes requests by id: `WorkerEnvelope` / `WorkerLine` / `WorkerEvent`
(`spawn_worker/wire.rs`), a thread per request writing under one lock (`spawn_worker/serve.rs`), and
`SpawnClient` as a router with per-request channels and `clone_repo_observed` (`spawn_worker/client.rs`).
`clone_as_user_observed` reports the `git clone` child's pid; worker pipes are close-on-exec; a dead
worker fails every pending request naming its pid. `spawn_worker.rs` went from 481 to 349 production
lines. See [the spawn worker](../spawn-worker.md).
