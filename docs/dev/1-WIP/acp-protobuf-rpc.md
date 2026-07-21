# Changeset: ACP as a protobuf RPC (`AcpService`) over the LiveKit session connection

**PRD**: `docs/ft/coder/acp-protobuf-rpc.md`
**Branch**: `feat/acp-chat-1`
**Packages**: `tddy-service`, `tddy-web`

Map the Agent Client Protocol (ACP) 1:1 onto a protobuf RPC and drive it over the *same*
transport-agnostic `tddy_rpc` surface that already carries `TddyRemote` — i.e. the **LiveKit session
connection**. A browser (or any `tddy_rpc` client) can now speak ACP directly to a session. Message,
field and enum names mirror `agent-client-protocol` verbatim so a JSON-RPC ACP peer translates
mechanically. Additive: `TddyRemote` and the JSON-RPC `tddy-coder --acp` agent are both kept.

Architecture: `web AgentChat (acp) --AcpService.Session / LiveKit--> session process
(MultiRpcService: TddyRemoteServer + AcpServiceServer) --> Presenter view`.

## Checklist

- [x] Create changeset + PRD
- [x] `acp.proto` — bidi `Session(stream AcpClientMessage) returns (stream AcpAgentMessage)`,
      envelope `id` correlation, 1:1-named payloads (`SessionUpdate`, `ContentBlock`, `ToolCall`,
      `PermissionOption`, `StopReason`, `SessionModelState`, …)
- [x] Codegen: `tddy_codegen` RpcService pass in `tddy-service/build.rs` + `lib.rs` include →
      `AcpServiceServer` (`NAME = "tddy.acp.v1.AcpService"`)
- [x] `convert_acp.rs` — prost↔internal mapping (parallel to `tddy_acp::mapping`, no `acp::` dep)
      with 8 unit tests
- [x] `TddyAcpService` (`service_acp.rs`) — Presenter view-adapter; `Session` handler pumps
      `AcpClientMessage`→intents and `PresenterEvent`→`AcpAgentMessage`
- [x] Mount `AcpServiceServer` beside `TddyRemoteServer` in `session_view_adapter_surface`
- [x] Runtime acceptance test (`integration_tests.rs`): a real `AcpService/Session` bidi call over
      the surface handshakes `initialize` and streams a live `AgentOutput` as an `AgentMessageChunk`
- [x] Web: `buf generate` → `src/gen/tddy/acp/v1/acp_pb.ts`
- [x] Web: `useAcpSession` hook (mirrors `useAgentChat`, returns `UseAgentChatResult`) driving
      `AcpService.Session`
- [x] Web: `AgentChat` gains an `acp` prop selecting `useAcpSession` vs `useAgentChat`
      (presentation shared via `AgentChatView`; neither hook called conditionally)
- [x] **Agent-initiated `request_permission` emit**: `TddyAcpService` maps
      `ModeChanged(Select/MultiSelect)` → an outbound `RequestPermission` (agent-allocated id); the
      client's reply decodes back to an `AnswerSelect` intent. Runtime test
      `acp_session_maps_a_select_clarification_to_a_request_permission`.
- [x] **Cypress** `AgentChatAcpStreamingAcceptance.cy.tsx` (2 tests: streamed `AgentMessageChunk`
      merge + full permission select round-trip over a mocked `AcpService`); the 8 existing
      `AgentChat*` specs stay green (the `AgentChatView` refactor is non-regressive).
- [x] **Per-turn `EndTurn`**: surfaced the presenter's `awaiting_open_answer` on `ModeChangedDetails`
      (internal-only; TUI/`TddyRemote` ignore it). `TddyAcpService` emits `PromptResponse(EndTurn)` at
      the free-prompting turn boundary (`ModeChanged` with the flag + a pending prompt id). Runtime
      test `acp_session_emits_end_turn_at_a_free_prompting_turn_boundary`.
- [x] **LiveKit-testkit end-to-end**: `packages/tddy-livekit/tests/acp_session_livekit.rs` drives
      `AcpService.Session` over a **real LiveKit hop** (server participant mounts
      `session_view_adapter_surface`; client uses `RpcClient.start_bidi_stream`): `initialize`
      handshake + a live `AgentOutput` → `AgentMessageChunk`. **Passes** against the testkit server.
- [x] **Maximal parity** (`convert_acp` + `service_acp::OutboundState`): every real signal the
      view-adapter sees is now mapped — `UserPrompt`→`user_message_chunk`,
      `Info`/`GoalStarted`/`StateChanged`→`agent_message_chunk`, a **tool-call lifecycle** (stable
      `tool-{n}` ids + `tool_call_update(Completed)` on the next call / turn end +
      `tool_call_update` progress), and a **synthesized `Plan`** accumulated from `TaskStarted` (prior
      entries flip to `Completed`). Unit + `OutboundState` + runtime tests.

## PR-Stack chat switched to ACP (behavior parity)

`PrStackChat` now passes `acp`, so the pr-stack chat drives the workflow over `AcpService.Session`
(not `TddyRemote.Stream`). ACP is leaner than the Presenter surface, so parity is achieved via
documented **tddy conventions** (both ends are ours) in `convert_acp` + `useAcpSession`:

- [x] goal → `agent_thought_chunk` → "goal" bubble
- [x] non-tool activity/system log line → one-shot `tool_call` → "activity" bubble
- [x] multi-select clarification → `request_permission` with a `clarification:multi` tool-call id;
      the question text + header ride the tool-call fields (`title` = question, `raw_input` = header)
- [x] answer encodings in the reply `option_id`: `option-{i}` → AnswerSelect, `other[:text]` →
      AnswerOther, `multi:{i,j}[;other=text]` → AnswerMultiSelect (decoded in
      `permission_response_to_intent`)
- [x] `user_message_chunk` is ignored by the web (the operator's message is already echoed locally)
- [x] all pr-stack chat Cypress specs migrated from `TddyRemote` mocks to a shared `acpSession`
      helper (`cypress/support/rpc/acpSession.ts`); **8 PrStackChat\* + 4 AgentChat\* specs green**

## Intentionally NOT mapped (documented; no data / not applicable)

- `agent_thought_chunk` — Claude's `thinking` blocks are parsed then discarded upstream
  (`stream/claude.rs`), so there is no thought data to emit; capturing it is a separate parser change.
- `available_commands_update`, `current_mode_update` — no clean internal source (`AppMode` ≠ ACP
  session mode).
- `fs/*`, `terminal/*` — client-provided capabilities an ACP *agent calls*; our workflow does its own
  file/shell I/O via the coding backend and never calls them. Left as name-mirrored stubs in
  `acp.proto`.

## Deviation from plan (WS3)

The plan proposed refactoring the JSON-RPC `tddy-coder --acp` agent to share `TddyAcpService`'s core.
**Not done, by design**: the standalone `--acp` agent is driven by an *external* host (Zed) and runs
a fresh `WorkflowEngine` per prompt — it has **no Presenter**, whereas `TddyAcpService` is a Presenter
view-adapter. They cannot share one execution core. The genuinely shared concern is the
internal↔ACP *mapping*, already factored into two parallel modules (`tddy_acp::mapping` in `acp::`
types for the JSON-RPC edge, `convert_acp` in prost for the RPC), kept semantically in lockstep
(option ids `option-{i}`/`other`, stop reasons, content shapes) and pinned by mirrored unit tests.

## Verification

- `./test -p tddy-service` — 48 tests pass (8 mapping + 1 ACP runtime acceptance + existing surface
  suite); `tddy-coder` builds with the mounted surface.
- Web: `bun run generate` regenerates `acp_pb.ts`; `useAcpSession`/`AgentChat` are `tsc`-clean.
