# 2026-10-06 — Every polling wait of the Rust backend beats a stage and a clock

**Type:** Feature

`backends/rust/wait.rs` (new) holds `WaitStage`, `Waiting` and the pure `heartbeat_line`; `WAIT_HEARTBEAT`
is 30 s. The five polling waits (`readiness.rs::ensure_indexed`, `readiness.rs::await_answer`, the assist
wait, `settled_outline`, `locate_symbol`) each own a `Waiting` and beat through the run's `progress`
sink; `keep_waiting`'s 100 ms slice checks the beat beside the cancellation token. `LspClientBridge::request`
waits in heartbeat-sized slices so a request in flight is narrated. `ServerChatter::quiet_for` is the time
since the server last said something new. `RestructureError::IndexingIncomplete` gains `stage`; the cancel
message reads `… after {n}s while {stage} (last progress: …)`. `Options.wait_heartbeat`,
`registry_for_waiting` and `RustBackend::with_wait_heartbeat` carry the cadence (a constant in production,
an injected `Duration` for tests).

`runner/compile_gate.rs` narrates a `cargo check` that does not finish, with its pid; the tidy
(`runner/tidy.rs`) and the group gate (`runner/group_gate.rs`) are the two recorded partial cuts, run
through a discarding sink meanwhile (`TODO(apply-heartbeat)`).

`tests/bin/fake_lsp.rs` (in `tddy-lsp`) gains the three busy modes every acceptance test drives; the new
suites are `tests/wait_heartbeat_acceptance.rs` and the unit tests in `wait.rs` and `chatter.rs`.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-apply-heartbeat.md).
