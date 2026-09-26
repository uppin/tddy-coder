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
and a `tool_calls` list.

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
