# Changeset: Session Activity — StreamMode flag + structured (Value) payload

**PRD**: `docs/ft/web/agent-activity-pane.md`
**Branch**: `fix-session-activities-perf`

## Summary

Two optimizations to `StreamSessionActivity` (already a per-tool-call server-streaming RPC):

1. **`StreamMode` flag** on `StreamSessionActivityRequest` — `SNAPSHOT_THEN_LIVE` (default,
   back-compat) replays the coalesced history then tails live; `LIVE_ONLY` skips the snapshot and
   delivers only records arriving after subscribe. Honoured identically by both hosts (daemon gRPC
   and coder LiveKit participant).
2. **Structured `google.protobuf.Value` `input`/`result`** replacing the opaque `input_json` /
   `result_json` strings — on the wire, in the persisted `agent-activity.jsonl`, and in the web
   detail dialog. `Value` (not `Struct`) because tool output is frequently a bare string/array/
   scalar, which an object-only `Struct` cannot represent.

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit/integration tests
- [x] `StreamMode` enum + `mode` field in proto; regenerate Rust + TS bindings
- [x] `AgentActivityRecord.input`/`result` → `google.protobuf.Value` in proto
- [x] Core `AgentActivityRecord` fields → `serde_json::Value`; append/read round-trip
- [x] Daemon: mode gate + `to_proto` Value mapping + `report_agent_activity` string→Value parse
- [x] Coder participant: mode gate + `to_proto` Value mapping
- [x] Presenter + sandbox capture seams build structured records
- [x] Web: `useSessionActivity(mode?)`, overlay default, detail dialog renders `Value`
- [x] All tests passing

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | `import "google/protobuf/struct.proto"`; add `enum StreamMode { SNAPSHOT_THEN_LIVE = 0; LIVE_ONLY = 1; }`; add `StreamMode mode = 4` to `StreamSessionActivityRequest`; change `AgentActivityRecord.input_json`/`result_json` (string) → `google.protobuf.Value input = 3` / `result = 5` |
| `packages/tddy-core/src/agent_activity.rs` | Struct `input_json`/`result_json: String` → `input`/`result: serde_json::Value`; `append`/`read` keep JSONL (un-nested object/scalar); migrate existing tests to `Value` |
| `packages/tddy-core/src/presenter/presenter_impl.rs` | Where it builds `AgentActivityRecord` (~275–326): parse the stream event's `input_json`/`result_json` strings into `serde_json::Value`; migrate its test (~1790–1815) |
| `packages/tddy-daemon/src/connection_service.rs` | `stream_session_activity` (6017): gate snapshot replay on `req.mode`; `to_proto_agent_activity` (342): map `serde_json::Value` → `prost_types::Value`; `report_agent_activity` (5725): parse hook `input_json`/`result_json` strings → `Value` when building the record |
| `packages/tddy-daemon/src/sandbox_session.rs` | Build `AgentActivityRecord` with structured `Value` input/result (parse from the executor's JSON) |
| `packages/tddy-coder/src/session_participant/mod.rs` | `"StreamSessionActivity"` arm (408): decode `mode`, gate snapshot; `to_proto_agent_activity` (478): `Value` → `prost_types::Value`; migrate its tests (~558–645) |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerated from the proto (buf) — `StreamMode`, `mode`, `input`/`result` as `Value` |
| `packages/tddy-web/src/components/sessions/useSessionActivity.ts` | Accept optional `mode` (default `SNAPSHOT_THEN_LIVE`), pass it on the request; expose records with structured `input`/`result` |
| `packages/tddy-web/src/components/sessions/AgentActivityOverlay.tsx` | Detail dialog renders `input`/`result` from the `Value` (pretty-printed JSON) instead of the raw `inputJson`/`resultJson` strings |
| `packages/tddy-web/cypress/component/AgentActivityAcceptance.cy.tsx` | Record builder emits structured `Value`; add structured-render + mode tests |
| `packages/tddy-web/cypress/support/pages/agentActivityPage.ts` | Helpers unchanged in shape; detail input/output assertions target rendered `Value` |

## Files to create

| File | Purpose |
|------|---------|
| _(none)_ | All changes land in existing files; tests extend existing suites/modules. |

## Design decisions

### `google.protobuf.Value`, not `Struct`
`tool_result_content_to_json` (`stream/claude.rs:86`) returns bare strings for string tool output;
results are often non-objects. `Struct` holds only a JSON object, so it cannot carry a scalar/array
result without a lossy wrapper convention. `Value` is the full JSON superset and represents every
shape faithfully. `prost-types 0.13` is already a `tddy-service` dependency, so no new proto-crate
dep; daemon/coder construct `prost_types::Value` (may add `prost-types` to those crates — a small,
in-ecosystem dep).

### Back-compat via the enum zero-value
`StreamMode::SNAPSHOT_THEN_LIVE = 0` is the proto3 default, so an old client that omits `mode`
keeps today's snapshot-then-live behavior. The web overlay keeps this default so the pane still
populates on open; `LIVE_ONLY` is an explicit opt-in.

### Write side stays string; server parses once
The `ReportAgentActivity` hook keeps sending `input_json`/`result_json` as strings (the hook
receives JSON text from Claude Code). The daemon parses them into `serde_json::Value` when building
the record. A non-JSON input string is stored as a JSON string scalar (`Value::String`) — no
fabrication, no data loss. This confines the contract change to the read/stream path.

### On-disk JSONL format change
Records persist with un-nested `input`/`result` (a JSON object/scalar) instead of a JSON-string-of-
JSON. Legacy rows carrying the old `input_json` string field fail to deserialize into the new struct
and are skipped on read (already the malformed-line behavior). Acceptable for ephemeral per-session
logs.

## Acceptance tests

Cypress component (`packages/tddy-web/cypress/component/AgentActivityAcceptance.cy.tsx`), mounted
over `anInMemoryRpcBackend` + `mountWithRpc` (no live daemon), plus a hook probe for mode:

1. **renders the structured tool input in the detail dialog** — a record whose `input` is a
   structured object `{ command: "cargo test --workspace" }` shows `cargo test --workspace` in the
   input pane. Validates the web consumes a `Value`, not a JSON string.
2. **renders a bare-string tool result in the detail dialog** — a record whose `result` is the
   bare string `"test result: ok. 412 passed"` shows that text in the output pane. Validates
   `Value` (not `Struct`) carries non-object output.
3. **subscribes with snapshot-then-live mode by default** — the overlay's `StreamSessionActivity`
   request carries `mode == SNAPSHOT_THEN_LIVE`. Guards the default + back-compat wiring.
4. **requests live-only mode when the hook is asked to** — a hook probe mounting
   `useSessionActivity({ mode: LIVE_ONLY })` sends a request with `mode == LIVE_ONLY`. Validates
   the web opt-in path exists.

(The existing icon-visibility, row-listing, unread-badge, and sandbox-session tests migrate to the
structured-`Value` record builder; behaviour unchanged.)

## Unit / integration tests

### Core — `packages/tddy-core/src/agent_activity.rs`
1. **append then read round-trips a call with a structured object input and result** — a record
   with `input`/`result` as JSON objects survives the JSONL round-trip unchanged.
2. **append then read round-trips a bare-string tool result** — `result = Value::String("…")`
   round-trips (the `Value` case a `Struct` could not represent).
3. Existing coalesce / malformed-line / tail-cap tests migrated to `Value` fields (behaviour
   unchanged).

Extended in the existing `agent_activity_unit_tests` module (reuses `make_unit_service`,
`write_claude_cli_session`, `a_pre_tool_use`, `a_seeded_record`, `next_record`):

4. **live-only mode skips the snapshot and delivers only records arriving after subscribe** — a
   snapshot row on disk, subscribe with `LIVE_ONLY`, publish a live record: the first (only) record
   delivered is the live one, not the snapshot.
5. **`report_agent_activity` parses the hook's JSON input into a structured record** — after a
   `PreToolUse` report, the persisted record's `input` is the parsed object (asserted on the core
   `serde_json::Value` field).
6. **a non-JSON input string is stored as a `Value::String` scalar** — no data loss, no fabrication.

(The existing `stream_session_activity_replays_the_persisted_snapshot` /
`..._delivers_a_live_record_after_the_snapshot` tests migrate to set `mode: SNAPSHOT_THEN_LIVE` and
become the back-compat / snapshot-then-live coverage.)

### Coder participant — `packages/tddy-coder/src/session_participant/mod.rs` (existing `tests` module)
7. **`StreamSessionActivity` in live-only mode skips the persisted snapshot** — `handle_rpc` with a
   `LIVE_ONLY` request over an `agent_activity_dir` holding a row; the first delivered frame is a
   record broadcast after subscribe, not the snapshot. (The existing snapshot-then-live test stays
   as the default-mode coverage, migrated to structured `Value`.)

### Presenter — `packages/tddy-core/src/presenter/presenter_impl.rs`
8. Existing agent-activity coalesce test migrated: the presenter builds an `AgentActivityRecord`
   whose `input`/`result` are structured `Value`s parsed from the stream event's JSON strings.

### Wire mapping (`to_proto_agent_activity`, both hosts)
Consolidated into a single shared `tddy_service::agent_activity_to_proto` (see Validation Results),
directly unit-tested via `tddy_service::json_to_proto_value` tests and end-to-end via the web
acceptance tests.

## Validation Results

**Review** (correctness / test-quality / production-readiness / clean-code): no blocking issues.
`StreamMode::try_from(...).unwrap_or(SnapshotThenLive)` degrades an unknown mode safely on both
hosts; `Null`→unset mapping is symmetric; number precision is inherent to `google.protobuf.Value`
(IEEE-754 double) and noted in the PRD; no new `unwrap`/TODO/mock/dead code.

**Refactor applied** (from review):
- Extracted the string→`Value` parser (was copy-pasted 3×) into one `tddy_core::agent_activity::parse_activity_json`; callers in daemon `connection_service.rs`, `sandbox_session.rs`, and core `presenter_impl.rs` now share it.
- Consolidated the two identical `to_proto_agent_activity` copies into one `tddy_service::agent_activity_to_proto`; daemon and coder participant call it.
- Added 7 `json_to_proto_value` unit tests in `tddy-service` (null→unset, string, object, array, bool, number, nested-null preserved).
- Added the `google.protobuf.Value` double-precision caveat to the PRD.

**Verified** (`./dev cargo`, `--lib`): `tddy-service` 68 passed, `tddy-core` agent_activity 10 passed, `tddy-daemon` agent_activity 9 passed, `tddy-coder` session_participant 27 passed; `cargo clippy -- -D warnings` clean; `cargo fmt --check` clean.

**Known pre-existing failures (not this change, present on `master`):** the daemon
`sandbox_session::dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client` test needs
`tddy-sandbox-runner` built first (passes under `./test`); `tddy-coder` `--all-targets` clippy
fails compiling `tests/session_catalog_populate.rs` (missing `catalog_provider`).
