# oversized-file: cli_agent_steps.rs

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs`
**Category:** oversized-file
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **651 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.30× over**
**Thresholds breached:** length 571 > 500
**Status:** Open — regressed 2026-11-06 by #825 (648 → 651) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 571 | first detection — #820 added the module: the generic launch's spec-keyed steps (slots, ssh slot writes, conversation, role and jail refusals) and the session wiring on a start and a resume. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 571 → 648 | **worse by 77** in #823 (`#agent-tools` 17/19): `resumed_or_adopted_conversation`, `carry_on_the_tool_server`, `hooks_report_over_the_agent_tool_socket`, and the session pointer on `SessionWiring`/`ResumedWiring` (the notify pairs routed to it). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-06 | 648 → 651 | **worse by 3** in #825 (`#agent-tools` 19/19): `SessionWiringInputs` gains `tddy_tools_path` (the host-resolved `tddy-tools`, replacing the daemon.yaml read) and the start records its hook answer wait (`record_hook_answer_wait`). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |

## What would close it

Split by seam with `code-restructuring`: the conversation (`LaunchConversation`, `ConversationStart`, `start_conversation`, `resumed_conversation`, `conversation_mint_refused`) into `cli_agent_steps/conversation.rs`, and the session wiring (`SessionWiringInputs`, `SessionWiring`, `wire_session_agent`, `ResumedWiring`, `resumed_session_wiring`, `route_tool_server_env`, `socket_for_hooks`) into `cli_agent_steps/wiring.rs`. Each half is under 300 lines.
