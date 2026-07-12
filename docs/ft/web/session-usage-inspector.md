# Session Usage — Real-time Token Usage in the Inspector

**Component:** `SessionInspectorDrawer` → new **Usage** tab (`packages/tddy-web/src/components/sessions/`)
**Updated:** 2026-07-12
**Status:** Planned

## Overview

Add a **Usage** tab to the Session Inspector showing a live, per-conversation token breakdown for
the selected session: the main agent and each subagent, with input / output / total tokens and
turn count, plus a session TOTAL row. Values update in near-real-time as the session runs.

Per-conversation token accounting already exists in the coder/sandbox layer
([Session Token Accounting](../coder/session-token-accounting.md)) but is only surfaced inside the
sandbox jail (MCP `subagent_list` + `accounting.json`) and printed to stderr at session end. This
feature carries that same `ConversationRecord` data over the daemon's existing session-event stream
to the web Inspector.

## Streaming design — no new endpoint

Token usage rides the **existing generic `ServerMessage` `oneof`** on `TddyRemote.Stream` — the
channel already used for session events (`AgentOutput`, `StateChanged`, …). A new
`TokenUsageUpdated` variant is added; older clients ignore unknown variants, so this is
backward-compatible. This is the deliberate extension point for adding further session-data events
over time (the reason we did not add a bespoke endpoint).

```protobuf
message ServerMessage {
  oneof event {
    // … existing 12 variants …
    TokenUsageUpdated token_usage_updated = 13;
  }
}

message TokenUsageUpdated {
  repeated ConversationRecord conversations = 1;  // full cumulative snapshot, never a delta
}

message ConversationRecord {
  string agent = 1;   // "claude", "Explore", "fastcontext", …
  string id = 2;
  string model = 3;
  uint64 input_tokens = 4;
  uint64 output_tokens = 5;
  uint64 total_tokens = 6;
  uint32 turns = 7;
}
```

### Snapshot semantics

`TokenUsageUpdated` always carries the **full cumulative list** of conversations, never a delta.
The client simply replaces its held snapshot on each event — idempotent and safe against broadcast
lag. Because the presenter stream sends no state snapshot on connect, the daemon emits the current
usage snapshot once when a subscriber connects, then a fresh snapshot on every transcript change, so
an idle session still shows its running totals immediately.

## Live source — file-watch / tail

The daemon watches the session's on-disk token sources and re-emits `TokenUsageUpdated` on change:

- **Main `claude` agent** — the transcript JSONL (`read_claude_transcript_usage`).
- **Claude Task subagents** — `<session_id>/subagents/agent-<id>.jsonl` (`read_claude_subagent_usages`).
- **tddy subagents** — the in-jail `accounting.json` (`TDDY_TOOLS_ACCOUNTING_FILE`).

These three are merged into one ordered `Vec<ConversationRecord>` by a reusable
`gather_session_usage(...)` (the merge currently inlined in `tddy-sandbox-app`'s
`print_token_summary`, extracted for reuse). Emissions are debounced; a missing transcript yields a
zero-token main-agent row, never an error.

## Layout

The Usage tab renders one row per conversation and a TOTAL row:

```
┌──────────────────────────────────────────────┐
│ Details | Tools | Usage | VNC | Screen Sharing│
├──────────────────────────────────────────────┤
│ agent      model            in     out   total  turns │
│ claude     claude-opus-4-8  12,340 3,210 15,550   7   │
│ Explore    claude-haiku     4,100    820  4,920   2   │
│ ────────────────────────────────────────────────────  │
│ TOTAL                       16,440 4,030 20,470       │
└──────────────────────────────────────────────┘
```

Before any usage arrives, the tab shows a zero/empty state (no conversation rows).

## Frontend

- `useSessionUsage(room, serverIdentity)` — opens `TddyRemote.Stream` (mirroring
  `usePresenterChat`'s client-build + `for await` pattern), keeps the latest
  `ConversationRecord[]` snapshot, ignores all non-`tokenUsageUpdated` events.
- `SessionUsageTab` — renders the breakdown table + TOTAL row; uses `formatTokens` for number
  formatting (mirrors `formatTraffic`).
- `InspectorTab` gains `"usage"`; `InspectorTabs` gains a Usage button; `SessionInspectorDrawer`
  renders `SessionUsageTab` for that tab.

## Scope

- **In scope:** `TokenUsageUpdated` on `ServerMessage`; daemon file-watch emitter; Usage tab.
- **Out of scope:** USD cost estimation; cache-token accounting; historical/persisted usage
  charts. Production LiveKit `Room` threading for the per-session presenter stream is tracked
  separately (see `usePresenterChat` TODO) and does not block this feature's tests.
</content>
