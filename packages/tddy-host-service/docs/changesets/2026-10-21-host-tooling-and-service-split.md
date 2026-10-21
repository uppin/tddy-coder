# 2026-10-21 — `host_tooling.rs` 806 ➜ 404 and `service.rs` 972 ➜ 242 production lines

**Type:** Refactor

`#host-desktops` 10/10 ([#693](https://github.com/uppin/tddy-coder/pull/693)). No behaviour change.

**`host_tooling.rs`** keeps the types, the probe trait, the argv builders, the parsers, `SubprocessHostToolingProbe`
and its `probe`, and the unit tests; three modules were moved out by the restructure engine (`move_item`):
`host_tooling/subprocess.rs` (184 — `ProbeOutput`, `StartedProbe`, `RunningProbe`, `start_probe`, the pipe
plumbing), `host_tooling/readings.rs` (165 — the agent probe and the git/gh/agent readings) and
`host_tooling/desktop_probes.rs` (100). `github_cli.rs` imports `start_probe`/`ProbeOutput` from `subprocess`.

**`service.rs`** keeps the struct, the stats-interval constants and the `HostService` trait impl. Its nine
`#[async_trait]` method bodies (21, 31, 39, 39, 49, 81, 43, 49 and 91 lines) were **moved by hand, with the
developer's consent**, into inherent `pub(crate)` `*_impl` methods behind one-line delegators, because the engine
cannot extract inside an `#[async_trait]` impl; each body was proved byte-identical by script against the commit
before. The engine then cut six modules: `service/builders.rs` (234), `service/peer_forwards.rs` (137),
`service/host_keys.rs` (251), `service/listings.rs` (135), `service/prompts.rs` (123), `service/stats_stream.rs`
(123), and extracted `os_user_for_session`, `await_passphrase_answer` and `add_key_on_blocking_pool` from
`add_host_key_impl` (86 ➜ 48 lines). `stream_host_stats_impl` stays at 96 lines, out of scope.

Widened by the moves: the readings and desktop-probe functions and the `StartedProbe::running` /
`RunningProbe::child` fields to `pub(super)`; the six peer-forwarding methods and `cached_tooling` to `pub(crate)`.
Hand fixes after engine applies, each a recorded engine defect: `pub(super) struct RunningProbe` (E0446, marked
`TODO`) and `#[cfg(unix)]` restored on the imports the moves re-pointed.

**Closed code issues** (records deleted with this change), final production lines, measured both to the first
`#[cfg(test)]` and excluding every `#[cfg(test)]` item (equal here):

- `oversized-file-host-tooling`: **806 ➜ 404**. History: 775 at detection (2026-10-15, #622), 775 (#656), 806 (#659).
- `oversized-file-service`: **972 ➜ 242** (no tests in the file). History: 939 at detection (2026-09-24, #509),
  934 (#579), 936 (#624), 972 (#656).

Resolved backlog entry: `2026-10-20-hosts-mount-service-rs-grew-while-over-budget` (#656's deferral of this split).
Baseline: 199 passed / 0 failed before and after; clippy `-D warnings` clean. Not touched:
`oversized-file-host-registry`, `complexity-host-messages-unlock-and-add`.
