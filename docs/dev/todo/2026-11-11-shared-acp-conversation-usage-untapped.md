# A shared-placement ACP conversation is registered for usage but never tapped

**Recorded:** 2026-11-11, by `#usage-binding 3/5` (PR #892).

## What is wrong

`tddy-session-agents::coding_agent::acp_usage_tap` builds the usage tap from
`CodingAgentLaunch::conversation_id`. That field names the *clone* worktree a conversation's exec
calls run in (`CONVERSATION_ID_ENV`), so it is `None` for a `shared`-placement conversation. Such a
conversation is still registered in `usage-conversations.jsonl` (its row appears, nested under the
main conversation), but its ACP `session/update` usage events are never written to
`<session_dir>/usage/<conversation_id>.jsonl`, so the row stays at zero.

Headless conversations are unaffected: their tap takes the conversation id directly.

## What would close it

A launch field of its own for the tap's conversation id (recommended), set for every placement,
leaving `conversation_id` to mean "the clone worktree" only. Not decided by the developer yet.

## Where

- `packages/tddy-session-agents/src/coding_agent.rs` — `acp_usage_tap`, `TODO(usage-binding)`
- `packages/tddy-coding-subagent/src/launcher.rs` — `CodingAgentLaunch::conversation_id`
