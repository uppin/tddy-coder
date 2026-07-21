# ACP as a protobuf RPC (`AcpService`)

**Status**: shipped — the browser drives a session over `AcpService.Session` at full behavior parity
with the Presenter path (streaming, tool-call lifecycle, synthesized plan, clarification round-trip,
per-turn `EndTurn`), verified end-to-end over a real LiveKit hop.

## What

`AcpService` is a protobuf mirror of the Agent Client Protocol (ACP), served over tddy's
transport-agnostic `tddy_rpc` surface — the same one that carries `TddyRemote`, i.e. the **LiveKit
session connection**. It lets any `tddy_rpc` client (notably the browser) drive a tddy session using
ACP semantics without a separate process hop or a bespoke wire.

The protobuf names mirror the `agent-client-protocol` crate **1:1** (`InitializeRequest`,
`SessionUpdate::AgentMessageChunk`, `ContentBlock::Text`, `PermissionOption`, `StopReason`, …), so a
standard JSON-RPC ACP peer (Zed, `claude-agent-acp`, `codex-acp`) translates to/from this RPC
mechanically.

## Why

Before this, ACP existed only as JSON-RPC over stdio (`tddy-coder --acp`), a standalone agent for
external hosts, while the browser reached the workflow over a *separate* `TddyRemote` stream. Putting
ACP on the shared `tddy_rpc` transport means:

- the agent is reachable as a first-class RPC on the LiveKit session — no extra stdio subprocess hop;
- the browser can speak ACP directly (one protocol, prost-encoded);
- external JSON-RPC ACP stays interoperable through a mechanical edge translation (1:1 names).

## Shape

- **Service**: `AcpService.Session(stream AcpClientMessage) returns (stream AcpAgentMessage)` — one
  bidi stream mirroring ACP's bidirectional JSON-RPC. Envelopes carry an application-level `id`
  (ACP request correlation), distinct from `tddy_rpc`'s own `(peer, request_id)` for the bidi call.
- **Client → Agent**: `initialize`, `authenticate`, `new_session`, `load_session`, `prompt`,
  `cancel`, and `request_permission` (the reply to an agent permission request).
- **Agent → Client**: the matching responses, plus `session_update` (streamed `SessionNotification`)
  and agent-initiated `request_permission`.

## Server (`TddyAcpService`)

A **Presenter view-adapter**, not a second workflow engine: it opens the same `ViewConnection` that
`TddyRemoteService` does, maps inbound ACP messages to `UserIntent`s, and maps outbound
`PresenterEvent`s to `AcpAgentMessage`s. Mounted beside `TddyRemoteServer` in
`session_view_adapter_surface`, so it rides every transport the surface does (LiveKit, gRPC, stdio).

Outbound mapping (`convert_acp` for the stateless 1:1 cases, `service_acp::OutboundState` for the
stateful lifecycle) covers everything the view-adapter can see: agent/user/informational text as
message chunks, a **tool-call lifecycle** (stable `tool-{n}` ids, `tool_call_update(Completed)` when
the next call opens or the turn ends, progress pings), a **synthesized `Plan`** accumulated from
workflow tasks, agent-initiated `request_permission` for clarifications, and `PromptResponse(EndTurn)`
at both `WorkflowComplete` and the free-prompting turn boundary (signalled by
`ModeChangedDetails.awaiting_open_answer`).

## Client (web)

`useAcpSession` (in `packages/tddy-web/src/components/chat/`) is the ACP counterpart of
`useAgentChat`, returning the identical `UseAgentChatResult`. `AgentChat` selects it via an `acp`
prop; both render the same UI (shared `AgentChatView`) and ride the same LiveKit room. **The
pr-stack chat (`PrStackChat`) uses ACP** (`acp`), at full behavior parity with the old `TddyRemote`
path — goal/activity/system bubbles, single- and multi-select clarifications, "other" free-text
answers, streaming, and error banners all work, via the tddy conventions below. The chat's "Export"
button (both transports) downloads a timestamped plain-text transcript that merges messages and
clarification points into one chronological timeline (`chatTranscript.ts`).

### tddy rendering conventions (over ACP)

ACP has no "goal"/"activity"/"multi-select-clarification" concepts, so these ride ACP fields (both
ends are ours; external ACP clients still get a sensible, if flatter, view):

- goal → `agent_thought_chunk`; non-tool activity → one-shot `tool_call`
- clarification → `request_permission`; `:multi` tool-call-id ⇒ multi-select; question + header ride
  the tool-call `fields` (`title` = question, `raw_input` = header); the "other" option ⇒ free-text
- answers ride the reply `option_id`: `option-{i}` / `other[:text]` / `multi:{i,j}[;other=text]`

## Relationship to the JSON-RPC `--acp` agent

`tddy-coder --acp` (JSON-RPC over stdio, for external hosts like Zed) is retained and unchanged. It
is a different execution context (external host drives a fresh `WorkflowEngine` per prompt, no
Presenter), so it does not share `TddyAcpService`'s core — only the internal↔ACP *mapping* concern is
shared, kept in lockstep across `tddy_acp::mapping` (acp:: types) and `tddy-service::convert_acp`
(prost).

## Intentionally not mapped

These ACP surfaces are name-mirrored in `acp.proto` but deliberately unhandled, because there is no
internal source or they don't apply to an agent:

- `agent_thought_chunk` — `thinking` blocks are parsed then discarded upstream (`stream/claude.rs`);
  surfacing them is a separate parser change.
- `available_commands_update`, `current_mode_update` — no clean internal source.
- `fs/*`, `terminal/*` — client-provided capabilities an ACP *agent calls* on the client; our
  workflow does its own file/shell I/O via the coding backend and never calls them.
