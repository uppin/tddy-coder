# 2026-09-28 — `roster/conversation.rs` crossed the file-length budget; split deferred past the stack

**Category:** Deferred — developer consented 2026-09-28 (PR #556 `/pr-wrap` step 3.5)

`packages/tddy-discovery/src/roster/conversation.rs` was 488 production lines — under the 500
budget — when PR #556 (`#subagent-control` 4/5) added the yield-condition parse framing and
crossed it (512, +24).

**Why deferred:** node 5 of the same stack (`resume-replacement`, #557) threads the same framing
surfaces (`turn_call` → `ResumeAgentConversationRequest`, the descriptor parse), so an
engine-driven extraction now would cascade conflicts through its diff. The developer chose defer
over decompose at wrap.

**What closing it would take:** after the `subagent-control` stack lands, extract the framing
concern (`parse_message_descriptor` + the final-chunk condition echo) from the conversation loop —
`code-restructuring`-driven, green baseline before and after. Measured record:
`packages/tddy-discovery/docs/code-issues/oversized-file-conversation.md`.
