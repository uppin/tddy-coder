# 2026-09-10 — `tddy-spawn`'s modules are over the file budget and were moved unsplit

**Category:** Deferred refactor
**Source:** `#unbundle` node 3, [#472](https://github.com/uppin/tddy-coder/pull/472), milestone M2

`tddy-spawn` was extracted from `tddy-daemon` with its four modules moved **verbatim** — git
recorded them as renames at 98–100% similarity. Two are well over the repo's 500-line budget:

| File | Lines |
|---|---:|
| `packages/tddy-spawn/src/spawner.rs` | 2,152 |
| `packages/tddy-spawn/src/spawn_worker.rs` | 568 |
| `packages/tddy-spawn/src/supervisor_spawn.rs` | 362 |
| `packages/tddy-spawn/src/supervisor_client.rs` | 79 |

Both were already over budget inside `tddy-daemon`; the move neither caused nor worsened it.

## Why they were not split during the move

Node 3's `## Boundaries` explicitly exempts file size ("Force every file under 500 lines" is out of
scope), and the exemption is load-bearing rather than convenient. The whole evidence that this node
relocated code rather than rewriting it *is* the near-100% rename similarity: `spawn_worker.rs` and
`supervisor_spawn.rs` are byte-identical, `supervisor_client.rs` differs by one import line, and
`spawner.rs` by eight. Splitting a 2,152-line file in the same commit would have destroyed that
signal and made the diff unreviewable as either a move or a refactor.

## What the refactor should look for

`spawner.rs` is the only real candidate, and it is not one concern:

- **argv and environment composition** — `pr_stack_spawn_args`, the `tddy_data_dir` resolution
  chain, `spawn_path_extra_for_home` threading. Pure functions over values; the natural first
  extraction and the most reusable.
- **LiveKit identity and room naming** — `resolve_livekit_room_name`,
  `livekit_spawn_daemon_instance_id`, `livekit_creds_from_config`,
  `livekit_server_identity_for_session`. Note `resolve_livekit_room_name` is the one symbol M2 had
  to widen from `pub(crate)` to `pub`, because three daemon call sites name the same room the spawn
  does — a hint that this cluster is a shared concern wanting its own module, not a spawner
  internal.
- **startup watch / timings** — `StartupWatch`, its `from_config` and the millisecond-pair form
  carried across the spawn-worker JSON IPC.

Do it **after** the `#unbundle` stack lands, against green tests, as its own change. Doing it while
nodes 4–8 are still rebasing on this branch would conflict every one of them.

## Related duplication, deliberately not addressed here

`packages/tddy-tools/src/server.rs:1584` `exec_tool_catalog()` is a verbatim hand-copy of
`packages/tddy-tool-engine/src/catalog.rs:16` `tool_catalog()`. Collapsing it belongs to
`#unbundle` node 8, where `tddy-tool-engine` gains the exec-tool service and becomes the single
owner; doing it earlier would leave the duplication half-resolved across two PRs. Node 3 keeps the
pair honest in the interval by **relocating rather than deleting** the guard test
`tool_catalog_sync.rs` (M4), which asserts the two catalogs still match.
