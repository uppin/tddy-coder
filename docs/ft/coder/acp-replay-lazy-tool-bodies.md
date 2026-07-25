# Lazy tool bodies in the ACP replay (`StreamAcpReplay` + `GetAcpToolCallDetail`)

**Status:** Implemented (Rust-only). Strips the heavy `raw_input`/`raw_output` payloads out of the
streamed transcript and moves them behind a unary, on-demand lookup.

## What

The read-only ACP transcript replay (`StreamAcpReplay`, see
[agent-activity-pane.md § Read-only ACP transcript](../web/agent-activity-pane.md#read-only-acp-transcript-evolution))
today streams each tool call **with its full body inline** — the ACP `ToolCall.raw_input` and
`ToolCall.raw_output`, each an arbitrary JSON string. This change:

1. **Always omits `raw_input`/`raw_output` from the transcript frames** in the
   `SNAPSHOT_THEN_LIVE` and `LIVE_ONLY` modes. A streamed tool call keeps its lightweight metadata —
   `title`, `status`, `kind`, `tool_call_id` — and nothing else. The list stays fully renderable;
   only the (potentially multi-MB) bodies are dropped.
2. **Adds a unary `GetAcpToolCallDetail`** that returns a single tool call's `raw_input`/`raw_output`
   on demand, resolved from the **same coalesced transcript view** the stream replays
   (`read_session_transcript`: `acp-transcript.jsonl` merged with the `agent-activity.jsonl`
   fallback). An operator clicking a tool-call row fetches that one call's body; the stream never
   pays for bodies nobody opens.

`COUNT_THEN_LIVE` is unchanged (it already carries no transcript payload — only `activity_count`).
`StreamSessionActivity` is out of scope.

## Why

Every tool call in a long session persists its full input **and** its full output into the
transcript (`raw_output` was added in fc990524 so the detail dialog could show it). Replaying a
whole session therefore streams the concatenation of **every tool's entire output** — file reads,
`Bash` stdout, search results — even though the operator only ever inspects one row at a time, and
often none. On a busy session that is the dominant cost of opening the pane, and it grows without
bound as the session runs. Oversized LiveKit RPCs are also a known failure mode: a single frame
above the chunking threshold can wedge the call silently.

Moving the bodies behind a per-call lookup makes the stream's size a function of the **number** of
tool calls (their metadata) rather than the **total volume** of their I/O, and defers the body cost
to the moment — if ever — an operator opens a specific row.

## Shape

### Stream frames lose their bodies

Both replay hosts already wrap each ACP `AcpAgentMessage` into an `AcpReplayFrame` before sending
it. A shared helper in `tddy-service::acp_replay` strips the body from a tool-call frame; both hosts
apply it at the wrap seam, so every snapshot **and** live frame is body-less in both modes:

```
tool_call frame in:  { tool_call_id, title, kind, status, raw_input: Some(..), raw_output: Some(..) }
tool_call frame out: { tool_call_id, title, kind, status, raw_input: None,     raw_output: None }
```

Non-tool frames (agent text) pass through unchanged.

### `GetAcpToolCallDetail` — unary body lookup

```protobuf
rpc GetAcpToolCallDetail(GetAcpToolCallDetailRequest) returns (GetAcpToolCallDetailResponse);

message GetAcpToolCallDetailRequest {
  string session_token      = 1;
  string session_id         = 2;
  string daemon_instance_id = 3;   // same peer-forward routing as StreamAcpReplay
  string tool_call_id       = 4;   // the ToolCall.tool_call_id from a streamed frame
}

message GetAcpToolCallDetailResponse {
  optional string raw_input  = 1;
  optional string raw_output = 2;
}
```

- The response bodies are the exact JSON strings the stream used to inline — resolved by scanning
  the session's coalesced transcript (`read_session_transcript`) for the frame whose
  `tool_call_id` matches, and returning its `raw_input`/`raw_output` (each may legitimately be
  absent — e.g. a still-running call has no output yet).
- A `tool_call_id` not present in the transcript is a **`NOT_FOUND`** error, not an empty success —
  so the caller can distinguish "no such call" from "call exists but has no output".
- `daemon_instance_id` routes exactly like the other per-session RPCs: served locally when it names
  this daemon (or is empty), forwarded to the owning peer otherwise (unary forwarding already
  exists).

### `tddy-service::acp_replay` helpers (the shared seam)

Both hosts read through the same module, so the strip and the lookup live there:

- **`strip_tool_body(frame: &AcpAgentMessage) -> AcpAgentMessage`** — returns the frame with a
  tool-call's `raw_input`/`raw_output` cleared (`title`/`kind`/`status`/`tool_call_id` retained);
  any non-tool-call frame is returned unchanged.
- **`tool_call_detail(session_dir, tool_call_id) -> io::Result<Option<ToolCallDetail>>`** — resolves
  the coalesced transcript and returns the matching call's `ToolCallDetail { raw_input, raw_output }`,
  or `None` when no frame carries that id. Because it reads the **same** `read_session_transcript`
  view the stream replays, the id an operator clicked is guaranteed resolvable to the same call the
  stream showed (latest state, deduped by `tool_call_id`).

## Hosting

Both `StreamAcpReplay` hosts strip at their existing frame-wrap seam and gain the new unary:

- **daemon `connection_service`** — serves dormant and daemon-hosted (claude-cli / sandbox)
  sessions. Strips in `acp_replay_frame` (covers the snapshot loop and the live
  `relay_acp_replay` tail). `get_acp_tool_call_detail` authenticates, resolves the session dir, and
  returns `tool_call_detail(...)`, mapping `None` to `NOT_FOUND`; peer-forwards on a foreign
  `daemon_instance_id`.
- **coder `session_participant`** — serves live tool/cursor sessions. Strips in `replay_frame_bytes`
  (covers snapshot + the live presenter tail). Adds a `GetAcpToolCallDetail` arm to its `handle_rpc`
  dispatch resolving `tool_call_detail(&self.svc.agent_activity_dir, ..)`.

## Scope

- **Rust-only.** The web consumption change (the detail dialog fetching a row's body lazily via
  `GetAcpToolCallDetail` instead of reading the now-absent inline body) is a **follow-up** and is
  not part of this changeset. Until then the web detail dialog reads bodies that the stream no
  longer carries — the wire change is additive (new RPC + fields), and the transcript-persistence
  seams are unchanged, so the web continues to decode frames; it simply sees empty
  `raw_input`/`raw_output` on streamed tool calls until it adopts the lookup.
- **`StreamSessionActivity` unchanged** — it never carried the ACP bodies.
- **`COUNT_THEN_LIVE` unchanged** — it already carries no transcript payload.
- Persistence is unchanged: `acp-transcript.jsonl` / `agent-activity.jsonl` still store the full
  bodies; only the **stream** is slimmed, and the lookup reads them straight back from disk.
