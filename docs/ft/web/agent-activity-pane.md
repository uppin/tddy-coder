# Agent Activity Pane — Real-time view of the agent's tool calls

**Component:** `AgentActivityOverlay` (new) in `packages/tddy-web/src/components/sessions/`, mounted in the `SessionMainPane` top bar
**Updated:** 2026-07-24
**Status:** Implemented (single-host; cross-host stream peer-forward is a tracked follow-up — see below). **Evolving** — the overlay body is being replaced by a read-only ACP transcript (see [§ Read-only ACP transcript](#read-only-acp-transcript-evolution)).

## Overview

Every session pane gains a top-right **activity icon** that opens an overlay listing **one-line
records of the agent's own tool calls** (Read, Shell/Bash, Edit, and `tddy-tools` verbs). Each row
expands into a **detail dialog** showing the tool call's **full input and full output** in a
scrollable view. The icon appears **only when the session has at least one recorded tool call**.
Records **stream in real time**, and **newly-arrived activity is flagged** with an unread badge
until the operator opens the overlay. The feature works for **every session type — tool,
cursor-cli, claude-cli, and sandbox**.

## Why

Today the web has no faithful view of what the agent did. The chat shows one-line "activity"
bubbles (title only, no input/output), and the one persisted per-session log (`tool-calls.jsonl`,
surfaced by `ListSessionToolCalls`) records **only human-triggered `ExecuteTool` invocations from
the Inspector — never the agent's own tool loop**. Operators need to see, live, the tool calls the
agent is actually making, with enough detail to debug.

## Data model — a new per-session agent-activity log

The agent's tool calls are captured into a new per-session JSONL log, `agent-activity.jsonl`
(sibling of `tool-calls.jsonl` in the session dir), distinct from the web-invoke log. One record
shape, shared across all hosts (defined once in `tddy-core`):

```
AgentActivityRecord {
  call_id           // stable id correlating the "running" and terminal rows
  tool_name
  input             // full tool input, as structured data (google.protobuf.Value)
  status            // "running" | "completed" | "error"
  result            // full tool output, as structured data; unset on the "running" row
  error_message     // non-empty when status == "error"
  started_unix_ms
  completed_unix_ms // 0 until terminal
  source            // "coder" | "cursor-cli" | "claude-cli" | "sandbox"
}
```

A tool call appends a `running` row when it starts and a terminal (`completed`/`error`) row when
it finishes (append-only keeps the write atomic). The read side **coalesces by `call_id`** (later
row supersedes, first-seen order preserved), then applies a 500-record tail cap. A crash mid-call
leaves a stuck `running` row → the UI shows it as in-progress.

**Structured input/output.** `input` and `result` are carried as **structured**
`google.protobuf.Value` values — the parsed JSON — both on the wire and in the persisted
`agent-activity.jsonl` log, not as opaque JSON strings. `google.protobuf.Value` is the full JSON
superset, so it faithfully carries an object input (`{"command": …}`), a bare-string result (tool
output is often a plain string), an array, a number, a bool, or null. Consumers render the value
directly rather than re-parsing a string. The `running` row leaves `result` unset. One wire-format
caveat: `google.protobuf.Value` numbers are IEEE-754 doubles, so integer tool values beyond 2^53
are represented approximately (inherent to the wire type).

## Capture (one seam per session type)

- **sandbox** — the daemon host-side executor `DaemonToolHandler::execute` appends running/terminal
  rows around `tool_engine::execute_tool_with_env`.
- **claude-cli** — Claude Code `PreToolUse`/`PostToolUse` hooks (via `tddy-tools session-hook`)
  carry `tool_name`/`tool_input`/`tool_response`; the hook POSTs a new `ReportAgentActivity` RPC and
  the daemon pairs Pre→Post per session (no shared id across hook processes).
- **tool / cursor-cli** — the coder presenter appends running/terminal rows as the agent's stream
  parser surfaces tool use + tool result (correlated by the existing `tool_use_id`).

## Streaming design — `StreamSessionActivity`

A new **server-streaming** RPC on `ConnectionService`:

```protobuf
message AgentActivityRecord { /* fields above */ }

// Whether the stream replays the persisted history before tailing live records.
enum StreamMode {
  SNAPSHOT_THEN_LIVE = 0;   // default: replay the coalesced history, then tail live records
  LIVE_ONLY          = 1;   // skip the snapshot; deliver only records that arrive after subscribe
}

message StreamSessionActivityRequest {
  string     session_token      = 1;
  string     session_id         = 2;
  string     daemon_instance_id = 3;   // same peer-forward routing as ListSessionToolCalls
  StreamMode mode               = 4;   // snapshot-then-live (default) vs live-only
}

// server-streaming: replays the coalesced history (snapshot) unless mode == LIVE_ONLY, then live deltas.
rpc StreamSessionActivity(StreamSessionActivityRequest) returns (stream AgentActivityRecord);

// unary; claude-cli hook → daemon.
rpc ReportAgentActivity(ReportAgentActivityRequest) returns (ReportAgentActivityResponse);
```

On connect the stream, when `mode == SNAPSHOT_THEN_LIVE` (the default, and the value a proto3
zero-field takes when the client omits it), **replays the coalesced snapshot** (so the
icon/history survive reconnect), then tails live records from an in-process per-session broadcast
hub. When `mode == LIVE_ONLY`, the snapshot replay is skipped entirely and the stream carries only
records that arrive after the subscription is established — a client that already holds the history
(or does not need it) avoids re-downloading up to 500 full records on every re-subscribe. In both
modes each streamed message is a single-tool-call delta. This mirrors the existing snapshot-then-live
`WatchTerminalControl` / `StreamTerminalOutput` pattern and the dual-host `ListSessionToolCalls`
(both daemon and coder participant read/write the same session dir); **both hosts honour `mode`
identically.**

- tool / cursor-cli: served by the **coder participant** over LiveKit while live; daemon over `/rpc`
  serves the file snapshot as fallback.
- claude-cli, sandbox: served by the **daemon** over `/rpc`.

**Shipped limitation:** `StreamSessionActivity` serves **Local** routes and rejects `PeerRoute::Forward`
with `unimplemented` (rather than serving wrong-host data) — `forward_to_peer` is unary-only, so a
streaming peer-forward primitive is a follow-up. Single-host, the common case, works fully.

## Frontend

- `useSessionActivity(sessionId, sessionToken, client, mode?)` — opens `StreamSessionActivity`
  (mirrors `useAgentChat`'s `for await` consumption), coalesces records by `call_id`, exposes
  `records`, `hasActivity`, `unreadCount`, and `markSeen()`. Opening the overlay marks the current
  records seen; records that arrive while the overlay is closed increment `unreadCount`. The
  optional `mode` selects `SNAPSHOT_THEN_LIVE` (default — the overlay keeps this so the list is
  populated on open) or `LIVE_ONLY`; the request always carries an explicit mode.
- `AgentActivityOverlay` — self-contained:
  - **icon button** (`agent-activity-button`, lucide, `variant="ghost"`) — rendered only when
    `hasActivity`; shows an **unread badge** (`agent-activity-unread-badge`) when `unreadCount > 0`.
  - **overlay pane** (`agent-activity-overlay`, mirrors `SessionInspectorDrawer`'s
    `absolute top-0 right-0 z-10`, `data-state` open/closed) — one row per record
    (`agent-activity-row-<callId>`) showing the tool name plus `[running]`/`[error]` markers.
  - **detail dialog** (`agent-activity-detail-dialog`, mirrors `SessionWorkflowFilesModal`:
    `fixed inset-0 z-50`, `role="dialog"`, Escape- and backdrop-close, scrollable `overflow-auto`
    body) — full structured `input` (`agent-activity-detail-input`) and `result`
    (`agent-activity-detail-output`), rendered from the `Value` (pretty-printed JSON).
- Wired into the `SessionMainPane` top bar next to the Inspector toggle, using
  `buildSessionClient() ?? client` (the same client selection the Inspector Tools tab uses).

## Layout

```
┌─ session pane top bar ─────────────────── [◱ activity •] [Inspector] ┐
│                                                                       │
│  (chat / terminal / sandbox view)      ┌─ activity overlay ─────────┐ │
│                                        │ Agent activity        [×]  │ │
│                                        │ ─────────────────────────  │ │
│                                        │ Bash                       │ │
│                                        │ Read                       │ │
│                                        │ Edit            [running]  │ │
│                                        └────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────┘

  clicking a row →  ┌─ detail dialog (scrollable) ─────────────┐
                    │ Bash                                [×]  │
                    │ Input                                    │
                    │ { "command": "cargo test", … }           │
                    │ Output                                   │
                    │ { "stdout": "…", "exit_code": 0 }         │
                    └──────────────────────────────────────────┘
```

The `•` on the icon is the unread badge. With zero records, no icon renders at all.

## Read-only ACP transcript (evolution)

The overlay's flat one-line-per-tool-call list is being replaced by a **read-only, ACP-style
conversation transcript** that interleaves the agent's text output with its tool calls, so an
operator sees *what the agent said and did*, in order — for a **live** session and, equally, a
**completed/dormant** one.

### What changes for the operator

- **Agent text is shown.** The transcript renders the agent's message output (from the persisted
  conversation transcript), not just tool calls. The old list showed tool calls only.
- **Tool calls carry inline detail.** Each tool call reads `Read main.rs L10-49`, `Bash cargo test`,
  `Glob **/*.rs` — the tool name **plus a short, per-tool detail** derived from its input — instead of
  a bare `Read`. A status marker (`running`/`error`) still applies.
- **DEBUG-style elapsed badge.** Each entry shows a colored `+Ns` / `+Nms` chip on its right edge —
  the wall-clock time elapsed since the previous entry — so slow steps stand out. This is computed
  from **real event timestamps** carried on the wire, so it is meaningful for replayed history too.
- **Read-only.** No message input, Send, or clarification controls — this is an inspection view.

### Why a new stream (`StreamAcpReplay`) rather than the existing RPCs

- The **literal ACP RPC** (`AcpService.Session`) is a *bidirectional* stream reachable in the
  browser only over **LiveKit**, which requires a live, attached session process. It has **no
  surface at all for a dormant session**, and even for a live one it forwards only events from
  connect-time (no history replay). It therefore cannot back an inspect-any-session view.
- **`StreamSessionActivity`** carries only tool calls (`agent-activity.jsonl`) — **no agent text** —
  so it cannot show the conversation. It is **kept as-is**, reserved for future *system* session
  messages, and no longer backs this overlay.

So a new **server-streaming** RPC emits **ACP-format frames** (reusing the `SessionUpdate` /
`ToolCall` / `AgentMessageChunk` data model), served over the same **HTTP** transport the overlay
already uses (no LiveKit, no Room), for **any** session:

```protobuf
// server-streaming: replay the session's persisted transcript as ACP session/update frames, then
// (SNAPSHOT_THEN_LIVE) tail the live presenter. Reuses the existing StreamMode enum.
rpc StreamAcpReplay(StreamAcpReplayRequest) returns (stream AcpReplayFrame);

message StreamAcpReplayRequest {
  string     session_token      = 1;
  string     session_id         = 2;
  string     daemon_instance_id = 3;   // same peer-forward routing as StreamSessionActivity
  StreamMode mode               = 4;   // snapshot-then-live (default) vs live-only
}

// The ACP `AcpAgentMessage` (session_update / error) as its protobuf bytes. Bytes keep
// connection.proto self-contained (no cross-package import / codegen); the web decodes them with the
// AcpAgentMessage schema and reuses the live ACP stream's session/update switch.
message AcpReplayFrame { bytes acp_agent_message = 1; }
```

Frames carry a timestamp on the ACP `SessionNotification` wrapper (`timestamp_unix_ms`), leaving the
ACP `SessionUpdate` oneof itself unchanged.

### Persisted ACP transcript — self-contained source of truth (Updated: 2026-07-24)

**The session persists its own ACP-mapped transcript** so the replay is self-contained and does
**not** depend on the coding agent keeping `conversation.jsonl` (that file is written by the agent
CLI and is not a contract we control). A new append-only per-session log, **`acp-transcript.jsonl`**
(sibling of `agent-activity.jsonl` in the session dir), holds one JSON `AcpAgentMessage`
(`session_update` frame) per line, **written at event time** by the same seam that captures
agent activity:

- The presenter maps each `PresenterEvent` through **`tddy-service::convert_acp`** + a persistent
  `OutboundState` (the *same* mapping the live ACP stream uses) and appends the resulting frame,
  **stamped with the real wall-clock `timestamp_unix_ms`**. Because history and live are produced by
  one mapper, a replayed transcript is byte-for-byte what a live viewer would have seen.
- Enriched tool detail (req #1) and the timestamp (req #2) are therefore computed **once, at write
  time** — `raw_input`/`kind`/`title` (tool detail via `tddy-core::stream::claude`) and the event
  timestamp are baked into the persisted frame. Replay does **no** re-derivation and **no** join.

Coverage matches the existing activity-capture seams: **tool / cursor-cli** sessions write the full
transcript (agent text + tool calls) from the coder presenter; **claude-cli / sandbox** write the
tool-call frames the daemon already captures (persisting agent *text* for those hook-driven types is
a follow-up — see Scope).

### Server-side replay (persisted transcript → ACP) (Updated: 2026-07-24)

The replay simply **reads `acp-transcript.jsonl` and re-emits its frames** (snapshot), then tails
live. It no longer parses `conversation.jsonl` or joins `agent-activity.jsonl` for the transcript —
those keep their existing roles. Hosting mirrors `StreamSessionActivity` end-to-end: the **coder
participant** replays the persisted snapshot then tails `presenter_events` (mapped + appended by the
same writer) for a live session; the **daemon** serves snapshot-only from the file for a dormant one
(the `presenter_events: None` path).

### Frontend

- `useAcpReplay({ sessionId, sessionToken, client })` — opens `StreamAcpReplay` over the HTTP client
  (mirrors `useSessionActivity`'s consumption), decodes `session_update` frames with the same switch
  as `useAcpSession`, and produces a richer read-only view model (`from: "user"|"agent"|"goal"|"tool"`,
  optional `tool: { name, detail, status, kind }`, and a real `at` timestamp).
- `AgentChatView` gains a `readOnly` prop: hides the input/Send/clarification UI, renders `tool`
  entries as an enriched card (`«name» «detail»` + status chip), and shows the right-edge `+Ns`
  elapsed badge per entry.
- `AgentActivityOverlay` swaps its data source to `useAcpReplay` and renders `<AgentChatView readOnly>`
  as the panel body (keeping the top-bar icon, unread badge, and open/close behavior).

## Scope

- **In scope:** the `agent-activity.jsonl` log + shared `AgentActivityRecord` (in `tddy-core`);
  capture seams for all four session types; `StreamSessionActivity` + `ReportAgentActivity` RPCs and
  their daemon + coder-participant hosts; the `AgentActivityOverlay` UI (icon, overlay, detail
  dialog, unread badge) wired into `SessionMainPane`. Also: the `StreamMode` request flag
  (snapshot-then-live vs live-only), honoured by both hosts; structured `google.protobuf.Value`
  `input`/`result` on the wire, in the persisted log, and in the web detail dialog.
- **In scope (Added: 2026-07-24):** the persisted **`acp-transcript.jsonl`** log (append-only,
  event-time ACP frames with real timestamps) as the self-contained source of truth for the
  transcript; the write-time ACP mapping at the coder presenter seam (tool / cursor-cli); the
  `StreamAcpReplay` RPC reading it (daemon snapshot-only + coder live-tail).
- **Out of scope (Added: 2026-07-24):** persisting agent **text** frames for the hook-driven
  **claude-cli / sandbox** session types (they persist tool-call frames only; agent-text capture for
  those is a follow-up); a retention/size cap on `acp-transcript.jsonl` (tracked with the
  `agent-activity.jsonl` cap in `docs/dev/TODO.md`).
- **Out of scope:** persisted-log row-size/result truncation cap (tracked in `docs/dev/TODO.md`);
  a plain `ListSessionAgentActivity` unary (add only if the stream proves heavy); filtering/search
  within the activity list; cross-session activity aggregation; splitting/chunking a single
  oversized record across multiple stream messages; changing the `ReportAgentActivity` write-side
  request to structured input/output (the hook keeps sending strings; the server parses them into
  the structured record). No cross-host stream peer-forward (still the tracked follow-up).
