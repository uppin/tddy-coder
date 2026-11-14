# 2026-11-14 — The `spawn-child` relay mapping leaves the toolcall client

**Type:** Feature

`method_for` no longer maps a `spawn-child` request to `SpawnChild`: `pr_spawn_child` is a
daemon exec tool, so no session relays a child spawn over its toolcall socket. The
`ChildSpawnHandler` trait, the `SpawnChild` wire arm and their plumbing stay for now — no
agent can reach them — and their retirement is recorded in
`docs/dev/todo/2026-11-14-retire-stack-child-spawn-handler.md`.
