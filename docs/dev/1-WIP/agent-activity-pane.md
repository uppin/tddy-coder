# Changeset: agent-activity-pane — per-session agent tool-call log, live stream, and overlay UI

**Date:** 2026-07-21
**Branch:** `feat-agent-activity`
**Packages:** `tddy-core`, `tddy-service` (proto), `tddy-daemon`, `tddy-coder`, `tddy-tools`, `tddy-web`
**Feature PRD:** [docs/ft/web/agent-activity-pane.md](../../ft/web/agent-activity-pane.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `tddy-core`: `AgentActivityRecord` + `agent_activity` log module (append / coalescing read / 500-cap)
- [x] `tddy-service`: proto `AgentActivityRecord`, `StreamSessionActivity`, `ReportAgentActivity`; regenerate Rust + `tddy-web/src/gen/connection_pb.ts`
- [x] `tddy-daemon`: `AgentActivityHub` (per-session broadcast + Pre→Post pending stack) + `StreamSessionActivity` (snapshot-then-live) + `ReportAgentActivity` handlers; stream-type binding in `connection_tonic_adapter.rs`
- [x] `tddy-daemon`: sandbox capture — `DaemonToolHandler::execute` appends running/terminal rows + publishes to hub
- [x] `tddy-tools`: `session-hook` POSTs `ReportAgentActivity` for `PreToolUse`/`PostToolUse` (fail-quiet) — parses tool fields via a private `HookToolPayload` (leaves `HookEvent`'s `Eq` intact, so the planned `session_activity.rs` field extension was intentionally avoided)
- [x] `tddy-coder`: enrich `ProgressEvent` (`input_json` + `call_id` on `ToolUse`; new `ToolResult` variant); presenter appends + broadcasts `PresenterEvent::AgentActivity`
- [x] `tddy-coder`: `"StreamSessionActivity"` dispatch arm in `session_participant/mod.rs`; `agent_activity_dir` + presenter broadcast wired in `run.rs`
- [x] `tddy-web`: `useSessionActivity` hook (stream subscription + coalescing + unread tracking)
- [x] `tddy-web`: `AgentActivityOverlay` (icon + unread badge + overlay pane + detail dialog)
- [x] `tddy-web`: wire `AgentActivityOverlay` into `SessionMainPane` top bar

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/AgentActivityAcceptance.cy.tsx` — 8/8 passing

## Unit / integration tests

- [x] `tddy-core`: `agent_activity` log module (round-trip, missing-file, malformed-line, 500-cap, running→completed coalescing) — 5 passing
- [x] `tddy-core`: `stream::claude` — a tool_use followed by its tool_result emits correlated `ToolUse` + `ToolResult`
- [x] `tddy-core`: `presenter` — `ToolUse` then `ToolResult` persists + broadcasts `AgentActivity`
- [x] `tddy-tools`: `session_hook` — pure Pre/Post mapping → `ReportAgentActivity` (4 tests, incl. non-tool event → no report)
- [x] `tddy-daemon`: `sandbox_session` — `DaemonToolHandler::execute` appends running + terminal rows (+ error row)
- [x] `tddy-daemon`: `connection_service` — `StreamSessionActivity` snapshot-then-live; `ReportAgentActivity` Pre→Post coalescing + bad-token rejection (5 tests)
- [x] `tddy-coder`: `session_participant` — `"StreamSessionActivity"` returns snapshot + a live `PresenterEvent::AgentActivity`

## Status notes

- **Cross-host streaming (`daemon_instance_id` peer-forward):** `StreamSessionActivity` serves Local
  routes and rejects `PeerRoute::Forward` with `unimplemented` (rather than serving wrong-host data)
  — `forward_to_peer` is unary-only; a streaming-forward primitive is a follow-up (TODO in
  `connection_service.rs`). Single-host (the common case) works fully.
- **Incidental baseline fix:** the `tddy-integration-tests` `output_parsing` target was failing to
  compile on the baseline (`missing field 'exploration' in PlanningOutput`, from the prior
  `exploration.md` slice #309, unrelated to agent activity). Fixed by adding `exploration: None` to
  the three `PlanningOutput` test literals; the target now passes (14 tests).

## Acceptance criteria

1. With zero recorded tool calls, **no** activity icon renders in the session pane top bar.
2. Once the session's first agent tool call is streamed, the activity icon appears.
3. Opening the overlay lists a one-line row per tool call (tool name + `[running]`/`[error]` markers).
4. Selecting a row opens a scrollable detail dialog showing the call's full input and full output.
5. The detail dialog closes on Escape and on backdrop click.
6. Newly-streamed activity is flagged with an unread badge until the overlay is opened.
7. The UI is session-type-agnostic — it renders the same for a **sandbox** session.

## Delta summary

### `tddy-core`
- **New:** `agent_activity` module — `AgentActivityRecord`, `append_agent_activity`, `read_agent_activity`
  (coalesce-by-`call_id`, first-seen order, 500-cap). Modeled on `tddy-daemon/src/tool_call_log.rs`;
  placed in `tddy-core` for a single cross-crate definition.
- **Modified:** `session_activity.rs` — `HookEvent` gains `#[serde(default)]` `tool_name`,
  `tool_input`, `tool_response`; a helper maps `PreToolUse`→running / `PostToolUse`→completed
  agent-activity (coarse `activity_status` mapping unchanged).
- **Modified:** `stream/mod.rs` — `ProgressEvent::ToolUse` gains `input_json: Option<String>`; new
  additive `ProgressEvent::ToolResult { call_id, result_json, is_error }`.

### `tddy-service`
- **Modified:** `proto/connection.proto` — `AgentActivityRecord`, `StreamSessionActivityRequest`,
  `StreamSessionActivity` (server stream), `ReportAgentActivity` unary. Regenerated Rust +
  `tddy-web/src/gen/connection_pb.ts`.

### `tddy-daemon`
- **Modified:** `connection_service.rs` — `AgentActivityHub`
  (`Mutex<HashMap<sessionId, broadcast::Sender<AgentActivityRecord>>>`); `StreamSessionActivity`
  (clone of `watch_terminal_control`: snapshot via `read_agent_activity`, then relay hub events with
  `Lagged` handling); `ReportAgentActivity` (append + publish, auth like `report_session_status`);
  `MpscAgentActivityStream`.
- **Modified:** `connection_tonic_adapter.rs` — bind `StreamSessionActivityStream`.
- **Modified:** `sandbox_session.rs` — thread `session_dir` + hub into `DaemonToolHandler`;
  `execute` appends running/terminal rows and publishes.

### `tddy-coder`
- **Modified:** `session_participant/mod.rs` — `"StreamSessionActivity"` dispatch arm (clone of
  `StreamTerminalOutput`): snapshot from `agent_activity_path` + subscribe to the presenter broadcast.
- **Modified:** `session_participant/connection_service_participant.rs` — `agent_activity_path` field.
- **Modified:** `presenter/presenter_impl.rs` — append `AgentActivityRecord` on tool-use/tool-result;
  broadcast `PresenterEvent::AgentActivity`.
- **Modified:** `run.rs` — build `agent_activity_path` beside `tool_calls_path`; pass the presenter
  broadcast into `SessionConnectionService`.

### `tddy-tools`
- **Modified:** `session_hook.rs` — for `PreToolUse`/`PostToolUse`, POST `ReportAgentActivity` with
  the tool payload; swallow errors and exit 0 (fail-quiet hook contract preserved).

### `tddy-web`
- **New:** `src/components/sessions/useSessionActivity.ts` — `StreamSessionActivity` subscription,
  coalesce-by-`call_id`, `records` / `hasActivity` / `unreadCount` / `markSeen()`.
- **New:** `src/components/sessions/AgentActivityOverlay.tsx` — icon button (hidden when no activity),
  unread badge, in-pane overlay list, full-input/output detail dialog.
- **Modified:** `src/components/sessions/SessionMainPane.tsx` — render `AgentActivityOverlay` in the
  top bar next to the Inspector toggle.
- **Test infra:** `cypress/support/testIds.ts` (+ `agent-activity-*` ids), new
  `cypress/support/pages/agentActivityPage.ts`.
