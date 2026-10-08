# 2026-10-09 — `tddy-session-lifecycle` becomes a wiring crate: every converted topic moves into its receiver

**Type:** Architecture

`#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)). The topic modules the in-place
conversion nodes (`#carve` 16a-16e, 17-20) made host-free moved out of `tddy-session-lifecycle` into the crates
that own their subject, by `tddy-tools restructure` engine moves, with no behaviour change. Four crates are
new: `tddy-agent-launch`, `tddy-session-split`, `tddy-cli-sessions`, `tddy-demo-vm-service`. Five existing
receivers gained code: `tddy-session-agents`, `tddy-session-files`, `tddy-session-activity`,
`tddy-daemon-livekit`, `tddy-daemon-kernel`. Lifecycle keeps the host, the builders, the port impls and
`PeerRouted*`, and re-exports every moved module at its old `tddy_session_lifecycle::…` path, so
`tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` name them as before. No receiver depends on
lifecycle. The layouts are in
[`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/docs/module-layout.md) and each receiver's docs.

## Before and after, per crate

Production lines by the node's counter (any line outside an inline `#[cfg(test)] mod … { }` block, `*_tests.rs`
excluded), and the tests each package runs (`./test -p <pkg>`, run one package at a time).

| Crate | Production lines before | after | Tests before | after |
|---|---:|---:|---:|---:|
| `tddy-session-lifecycle` | 20.3k | 5,587 | 658 passed, 22 known failures, 1 ignored | 564 passed, the same 22 known failures, 1 ignored |
| `tddy-agent-launch` (new) | 0 | 9,213 | 0 | 46 |
| `tddy-session-split` (new) | 0 | 3,361 | 0 | 37 |
| `tddy-cli-sessions` (new) | 0 | 1,738 | 0 | 9 |
| `tddy-demo-vm-service` (new) | 0 | 329 | 0 | 0 |
| `tddy-session-agents` | 4,132 | 6,606 | 75 | 75 |
| `tddy-session-files` | 4,716 | 5,143 | 160 | 160 |
| `tddy-session-activity` | 2,541 | 3,000 | 45 | 47 |
| `tddy-daemon-livekit` | 5,863 | 6,337 | 178 | 178 |
| `tddy-daemon-kernel` | 3,409 | 3,586 | 126 | 126 |
| `tddy-demo-runner` | 156 | 156 | 15 | 15 |
| **sum** | | | **1,257 passed** | **1,257 passed** |

The "before" lines of the receivers are the planning-time measurements. The kernel was re-measured after the
two path-reading tests were fixed (126 passed, 0 failed); the other crates are the 2026-10-08/09 runs. Every
receiver is under 10k production lines. One file in the new crates is over 500, `tddy-agent-launch`'s
`cursor_cli_spawn.rs` (563; 549 before the move), moved whole with its split deferred by the developer. The
largest remaining lifecycle file is `connection_service.rs` (525, from 582).

## Deferred, with the developer's approval

- **Lifecycle is 5.6k lines against a ~4.5k target.** The overshoot is the 366-line `test_util`, which is not
  gated behind a feature because the tests of `tddy-daemon` and `tddy-daemon-rpc` name it (a feature would
  edit their manifests), and the facade lines. Accepted when the developer asked for the wrap; tracked by
  `2026-10-08-session-lifecycle-test-util-is-not-gated-behind-a-test-util-feature`.
- **`cursor_cli_spawn.rs` is 563 lines**: `2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines`.
- **Other findings the moves surfaced**, in `docs/dev/todo/`: `2026-10-08-narrow-the-items-the-carve-moves-widened-to-pub-that-no-other-crate-uses`,
  `2026-10-08-demo-vm-service-impl-constructor-still-names-the-host` and its ruling update, and the
  `2026-10-08-restructure-*` entries on the engine's refusals during the moves. No `docs/dev/todo/` entry is
  resolved by this change; `2026-09-24-lifecycle-modules-to-re-parent-by-hand` is narrowed to the three modules
  that remain (`rpc_activity`, the `cli_spawn/` regrouping, `ManagedWorkflow`).

## Verification

- `cargo tree -i tddy-session-lifecycle -e normal,dev --workspace` lists `tddy-daemon`, `tddy-daemon-rpc`,
  `tddy-desktop`, `tddy-telegram-control` and the dev users `tddy-model-registry`, `tddy-tool-engine`,
  `tddy-worktree-service`: no receiver.
- No reverse edge between receivers: `tddy-session-split` does not reach `tddy-agent-launch`; the agents,
  cli-sessions and files crates reach none of launch, split or each other's forbidden sides.
- `restructure verify --against 468b368f9` (the base's tip): 371,133 statements before, 371,138 after, exit 1
  with 24 lost and 29 gained, all of them re-spelled paths with the rustfmt wrapping they cause, the four
  path strings of two source-reading tests, the `DemoVmServiceImpl::new(state)` constructor and its call
  sites, and the new crates' doc lines.
- Consumers: `cargo check --all-targets` clean on `tddy-daemon`, `tddy-daemon-rpc`, `tddy-telegram-control`,
  `tddy-model-registry`, `tddy-tool-engine`, `tddy-worktree-service`; the only consumer edits are the two
  `tddy-daemon` call sites of `DemoVmServiceImpl::new`. `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --check` clean on the eleven touched crates. `tddy-desktop` builds on CI only.
- Two tests that read moved files by path were fixed: `tddy-daemon-kernel`'s `telegram_extraction_shape` and
  `tddy-daemon-sandbox`'s `sandbox_session_stdio_acceptance`. On macOS two other targets of
  `tddy-daemon-sandbox` fail the same way on the base: the seatbelt stdio test times out, and
  `sandbox_stdio_seatbelt_acceptance` lacks a `SandboxHandle` import.

## Code issues

Ten of lifecycle's records moved with their code: seven to `tddy-agent-launch/docs/code-issues/` and three to
`tddy-session-split/docs/code-issues/`, each with a `**Moved:**` line and a re-measurement (every function is
the same length before and after the move; CRAP and coverage were not re-derived). Lifecycle keeps
`complexity-daemon-rpc-handler-handle-rpc` (185 lines, unchanged) and `oversized-file-connection-service`
(582 to 525 lines, still over 500). No record was closed, so none was deleted.
