# Changeset: acp-replay-lazy-tool-bodies — strip tool bodies from StreamAcpReplay + add GetAcpToolCallDetail

**Date:** 2026-07-25
**Branch:** `feature/lazy-activity-body/strip-stream-bodies`
**Packages:** `tddy-service`, `tddy-daemon`, `tddy-coder`
**Feature PRD:** [docs/ft/coder/acp-replay-lazy-tool-bodies.md](../../ft/coder/acp-replay-lazy-tool-bodies.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `connection.proto`: add `GetAcpToolCallDetail` rpc + `GetAcpToolCallDetailRequest` /
      `GetAcpToolCallDetailResponse` messages
- [x] `tddy-service::acp_replay`: `strip_tool_body(&AcpAgentMessage) -> AcpAgentMessage`
- [x] `tddy-service::acp_replay`: `ToolCallDetail { raw_input, raw_output }` +
      `tool_call_detail(session_dir, tool_call_id) -> io::Result<Option<ToolCallDetail>>`
- [x] `tddy-daemon connection_service`: strip in `acp_replay_frame` (snapshot loop + live
      `relay_acp_replay`); implement `get_acp_tool_call_detail` (auth + peer-forward + `NOT_FOUND`)
- [x] `tddy-daemon connection_tonic_adapter`: delegate `get_acp_tool_call_detail`
- [x] `tddy-coder session_participant`: strip in `replay_frame_bytes`; add `GetAcpToolCallDetail`
      arm to `handle_rpc`

## Acceptance tests

- [x] `packages/tddy-daemon/src/connection_service.rs` (`#[cfg(test)]`):
  - [x] `stream_acp_replay_snapshot_frames_omit_tool_bodies`
  - [x] `stream_acp_replay_live_frames_omit_tool_bodies`
  - [x] `get_acp_tool_call_detail_returns_the_full_tool_bodies`
  - [x] `get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id`
- [x] `packages/tddy-coder/src/session_participant/mod.rs` (`#[cfg(test)]`):
  - [x] `stream_acp_replay_snapshot_frames_omit_tool_bodies`
  - [x] `get_acp_tool_call_detail_returns_the_full_tool_bodies`
  - [x] `get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id`

## Unit tests

- [x] `packages/tddy-service/src/acp_replay.rs` (`#[cfg(test)]`):
  - [x] `stripping_a_tool_call_frame_drops_its_raw_input_and_raw_output`
  - [x] `stripping_a_tool_call_frame_keeps_its_title_kind_status_and_id`
  - [x] `stripping_a_non_tool_frame_leaves_it_unchanged`
  - [x] `tool_call_detail_returns_the_full_bodies_for_a_recorded_call`
  - [x] `tool_call_detail_returns_none_for_an_unknown_tool_call_id`

## Validation Results

### validate-changes (2026-07-25)
- **Critical: 0 · Warning: 0 · Info: 1.** All three crates build; `clippy --tests -D warnings` and
  `fmt` clean; all 12 new tests pass. No leftover `todo!`/`unimplemented`, no new `unwrap`/`expect`
  in production paths, no secrets/unsafe/TUI-stdout, no test-only branches.
- **Info:** web `connection_pb.ts` not regenerated — documented Rust-only follow-up (see § Scope).

### validate-tests / validate-prod-ready / analyze-clean-code (2026-07-25)
- **Tests:** all 12 follow Given/When/Then, one behavior each, named helpers, meaningful fixtures,
  exact-equality assertions; no anti-patterns. **Prod-ready:** no mock/hardcoded code, no leftover
  `todo!`/`FIXME`/TODO, no unused code. **Clean-code:** helpers small + single-purpose; the daemon
  unary mirrors the existing `execute_tool` routing/auth for consistency. No refactor needed.

### Final gates (2026-07-25)
- `cargo fmt --check` clean; `cargo clippy -p tddy-service -p tddy-daemon -p tddy-coder --tests
  -D warnings` clean.
- Tests: tddy-service **87/0**, tddy-coder **84/0**, tddy-daemon **354/1** — the one failure
  (`sandbox_session::…dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client`) is a
  pre-existing environmental failure requiring `tddy-sandbox-runner` built via `./test`, unrelated to
  this change. No regressions.

## Delta summary

### `tddy-service`

- **`proto/connection.proto`** — new unary `GetAcpToolCallDetail` on `ConnectionService`;
  `GetAcpToolCallDetailRequest { session_token, session_id, daemon_instance_id, tool_call_id }`
  and `GetAcpToolCallDetailResponse { optional raw_input, optional raw_output }`. Additive; regen
  Rust (`connection_pb.ts` regen for the web is a follow-up, Rust-only here).
- **`src/acp_replay.rs`** —
  - `strip_tool_body(&AcpAgentMessage) -> AcpAgentMessage`: clears `raw_input`/`raw_output` on a
    tool-call frame, keeps `title`/`kind`/`status`/`tool_call_id`; passes non-tool frames through.
  - `ToolCallDetail { raw_input: Option<String>, raw_output: Option<String> }`.
  - `tool_call_detail(session_dir, tool_call_id) -> io::Result<Option<ToolCallDetail>>`: scans
    `read_session_transcript` for the matching `tool_call_id`, returns its bodies or `None`.

### `tddy-daemon`

- **`src/connection_service.rs`** — `acp_replay_frame` now wraps `strip_tool_body(frame)` so both
  the `SnapshotThenLive` snapshot loop and the live `relay_acp_replay` tail emit body-less
  tool-call frames (`LiveOnly` uses the same relay). New `get_acp_tool_call_detail`: `record_rpc_activity`,
  peer-route on `daemon_instance_id` (forward via `forward_to_peer` / reject foreign), auth
  (`user_resolver` → `os_user_for_github`), resolve session dir, `tool_call_detail(...)`,
  `None` → `Status::not_found`. `COUNT_THEN_LIVE` path untouched.
- **`src/connection_tonic_adapter.rs`** — delegate `get_acp_tool_call_detail` to the inner
  `RpcConnectionService`.

### `tddy-coder`

- **`src/session_participant/mod.rs`** — `replay_frame_bytes` wraps `strip_tool_body(frame)` so the
  snapshot replay and the live presenter tail emit body-less frames in both modes. New
  `GetAcpToolCallDetail` arm in `handle_rpc` resolving
  `tool_call_detail(&self.svc.agent_activity_dir, &req.tool_call_id)`, mapping `None` to
  `Status::not_found`.

## Notes

- `COUNT_THEN_LIVE` and `StreamSessionActivity` are explicitly out of scope.
- Persistence (`acp-transcript.jsonl` / `agent-activity.jsonl`) is unchanged — full bodies stay on
  disk; only the streamed frames are slimmed and the lookup reads bodies back on demand.
- Web adoption of `GetAcpToolCallDetail` for the detail dialog is a follow-up (see PRD § Scope).
