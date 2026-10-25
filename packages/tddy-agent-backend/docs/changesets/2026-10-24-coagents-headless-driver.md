# 2026-10-24 — Headless coding agents on a conversation driver

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). New `backend/headless_driver.rs` (+ `argv.rs`, `stream_json.rs`, `codex_jsonl.rs`, `oz_ndjson.rs`, `transcript.rs`; re-exported): `HeadlessDriver` runs `claude -p`, `cursor agent -p`, `codex exec --json` and Warp's `oz agent run` once per turn, resuming the CLI's own session, and maps their streams to `AcpUpdate`s and an `AcpTurnEnd`. Cancel SIGKILLs the turn's process group. `ConversationDriver` is the trait both it and `AcpConversation` (an `AcpDriver` bound to one session) implement. Fixtures: real `oz` captures under `tests/fixtures/oz/`. Tests: `tests/headless_driver_acceptance.rs` (11), against fake CLIs.

Code issues: `oversized-file-backend-mod` 850 → 851 (`pub mod headless_driver;`); new `complexity-headless-driver-run-turn` (80 lines). Both deferred to the stack's sweep node. See [architecture](../architecture.md).
