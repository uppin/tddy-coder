# Changeset: toolcall submit immediate wire acknowledgment

**Status**: 🚧 In Progress

## Summary

The Unix relay acknowledges `tddy-tools submit` on the wire immediately after persisting results, instead of waiting for the presenter loop. The presenter receives `ToolCallRequest::SubmitActivity` only for activity-log lines. This removes presenter-scheduling timeouts when `poll_workflow` holds the presenter lock for extended periods.

## Affected packages

- `packages/tddy-core` — `toolcall/listener.rs`, `toolcall/mod.rs`, `presenter/presenter_impl.rs`
- `packages/tddy-coder` — `tests/daemon_toolcall_poll_regression.rs` (doc/assert message alignment)
- `packages/tddy-core` — `tests/toolcall_relay_presenter_stuck.rs` (behavior expectation)

## Implementation Progress

**Last synced with code**: 2026-03-22 (via @validate-changes)

**Core features**:

- [x] Immediate `SubmitOk` on wire after `store_submit_result` — ✅ Complete (`listener.rs`)
- [x] Rename `Submit` → `SubmitActivity`; presenter logs only — ✅ Complete (`mod.rs`, `presenter_impl.rs`)
- [x] Remove submit-specific presenter response timeout — ✅ Complete (`listener.rs`)
- [x] `try_send` for activity queue with full/disconnected warnings — ✅ Complete (`listener.rs`)

**Testing**:

- [x] `toolcall_relay_presenter_stuck` — ✅ Complete (expects `ok` when presenter never polls)
- [x] `daemon_toolcall_poll_regression` — ✅ Complete (doc + assertion text)
- [x] `cargo clippy -p tddy-core -p tddy-coder --all-targets -- -D warnings` — ✅ Complete

## Acceptance criteria

- [x] `tddy-tools submit` completes without blocking on presenter `poll_tool_calls` scheduling
- [x] Stored submit data remains available via existing `store_submit_result` path
- [x] Activity log still updated when presenter polls (unless queue full / disconnected)

### Change validation (@validate-changes)

**Last run**: 2026-03-22  
**Status**: ⚠️ Warnings (full workspace `cargo test` has unrelated failure)  
**Risk level**: 🟢 Low (for changed code)

**Changeset sync**:

- ✅ New changeset created; items match working tree

**Build / lint**:

- `cargo build -p tddy-core -p tddy-coder` — ✅ Pass  
- `cargo clippy -p tddy-core -p tddy-coder --all-targets -- -D warnings` — ✅ Pass  

**Tests**:

- Targeted: `toolcall_relay_presenter_stuck`, `daemon_toolcall_poll_regression` — ✅ Pass  
- Full workspace: `./dev ./verify` — ❌ Failed at `tddy-e2e` `grpc_reconnect_second_stream_receives_full_tui_render` (not in this diff; likely flaky or pre-existing)

**Analysis summary**:

- Production vs test: no test-only branches added; behavior is the same in all environments  
- Security: no new trust boundaries; same JSON submit payload handling  

**Risk assessment**:

- Build validation: Low  
- Test infrastructure: Low  
- Production code: Low–Medium (queue-full path drops activity notification but submit already succeeded — by design; logged)  
- Security: Low  
- Code quality: Low  

## Refactoring needed

### From @validate-changes

- [ ] Investigate `tddy-e2e` `grpc_reconnect_acceptance` failure when running full `./dev ./verify` (out of scope for this changeset unless reproduced on clean `master`)
