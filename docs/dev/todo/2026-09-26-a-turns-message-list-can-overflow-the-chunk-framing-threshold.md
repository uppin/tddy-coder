# 2026-09-26 — A turn's message list can push the final conversation frame past the framing threshold

**Category:** Defect — latent
**Source:** `2026-09-26-subagent-turn-control-and-honest-tool-failure` changeset, PR #545 —
`/validate-changes` finding W2

`agent_conversation_frames` (`packages/tddy-session-agents/src/service.rs`) deliberately caps each
frame's content at `HOST_DOCUMENT_FRAME_BYTES` = 48 KiB, and its own comment says why: over
LiveKit anything past `MAX_CHUNK_FRAME_BYTES` = 60 000 is chunk-framed, *"and one lost chunk frame
wedges the call with no error at all"*.

PR #545 added `agent_turn_frames`, which attaches `last.messages` — an **unbounded**
`repeated AgentMessageDescriptor` — to that same final frame. Each descriptor carries a 240-**char**
preview (`MESSAGE_PREVIEW_CHARS`), i.e. up to ~960 bytes in UTF-8, plus an id, a role, a tool name
and a `tool_calls` list. PR #553 adds a per-tool `resultSummary` on every tool-role descriptor —
bounded by construction (`packages/tddy-discovery/src/subagent/result_summary.rs`): the only text
is `firstLine`, cut to 120 chars (≤ ~480 bytes in UTF-8), plus a few counters, so a summary adds
at most ~600 bytes per tool-role descriptor. PR #556 (`yield-conditions`) adds one more field to
that same final frame: the fired condition a yielded turn reports (`packages/tddy-discovery/src/subagent/yield_condition.rs`),
echoed only when the turn stopped on a caller's condition, and bounded by `validate`'s own rules —
at most one condition (not all 8), whose largest part is a `contains` needle cut at 256 chars — so
at most a few hundred bytes, once.

At the ceiling of 50 turns a prompt can append well over a hundred messages. A hundred descriptors
is 30–100 KB on one frame, on top of up to 48 KB of content — so the invariant the existing
comment relies on no longer holds for the final frame, and the failure it names is a silent wedge
rather than an error.

Not yet observed: it needs a long turn over a LiveKit-carried conversation, and the local and MCP
paths do not frame.

## What closing it would take

The descriptors are the natural thing to page, since they are a list: either carry them on their
own frames after the content (the framing machinery already handles a multi-frame answer), or cap
the list the way the content is capped and say in the final frame that it was capped. Capping
silently would be the wrong answer here — a truncated message list is a rewind point a caller
cannot name, which is the failure mode `subagent_resume` exists to avoid.

Related: `docs/dev/todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`
— there is no wire-level test that would catch this either.
