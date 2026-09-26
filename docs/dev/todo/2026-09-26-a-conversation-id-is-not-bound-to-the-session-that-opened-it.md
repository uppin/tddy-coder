# A conversation id is not bound to the session that opened it

**Found:** 2026-09-26, during PR #545 review (`subagent turn control and honest tool failure`)
**Area:** `tddy-session-agents` — `SessionAgentService` conversation operations
**Severity:** authorization gap, pre-existing, not introduced by #545
**Status:** open, deliberately not fixed in #545

## What is missing

Nothing checks that the `conversation_id` a request names belongs to the `session_id` — or to the
session token — the request was authenticated with. A caller holding a valid session token and
*any* live conversation id is served, including a conversation opened by a different session on
the same host.

Two independent halves of the lookup, neither of which closes the gap:

- **`SessionAgentServiceImpl::session_dir`** — `packages/tddy-session-agents/src/service.rs:58-64`

  ```rust
  fn session_dir(
      &self,
      session_token: &str,
      session_id: &str,
  ) -> Result<std::path::PathBuf, Status> {
      (self.ports.session_dirs)(session_token, session_id)
  }
  ```

  The token is resolved to an OS user and the session directory is validated, but `session_id` is
  never cross-checked *against the token*. It selects which directory to work in; it does not
  prove the caller owns that session.

- **`OpenAgentConversations::routing_for`** —
  `packages/tddy-session-agents/src/agent_conversations.rs:106-129`

  A `Mutex<HashMap<String, AgentConversation>>` that is **host-global** and keyed on conversation
  id alone. Its own docstring is explicit about this: *"One map for the whole host rather than one
  per session: a conversation id is what a caller names in a prompt and a cancel, and the request
  carries the session it belongs to only so the status can be recorded against the right roster."*
  The routing comes back for whatever id matches, regardless of which session opened it.

`AgentConversation` does store its `session_id`, and the comparator a fix would use **already
exists**: `AgentConversation::is_with(session_id, agent_id)` at
`packages/tddy-session-agents/src/agent_conversations.rs:53-67`. Today only the detach path calls
it. A fix is therefore mostly a matter of routing every conversation operation through a lookup
that takes the session id and rejects a mismatch — plus deciding what `session_dirs` should assert
about the token/session pair, which is the harder half.

## Why it is not fixed here

It predates #545 by every operation it affects. `PromptAgentConversation` and
`CancelAgentConversation` were already on `IN_JAIL_RELAYABLE` and already reachable this way;
#545's widening to `ResumeAgentConversation` (5 → 6 entries) adds a sixth caller of the same
unbound lookup rather than creating the gap. Closing it means changing the signature or the
semantics of the shared conversation lookup for all six operations, with its own tests, on a PR
whose subject is turn budgets and tool-failure honesty.

The concrete harm #545 *does* change is worth naming: prompt adds a turn, whereas resume's
`from_message_id` **truncates** a transcript and `correction` injects into it. The same unbound
lookup now reaches a destructive write, not only an append.

## What #545 did instead

Three places justified the widening by an enforced `(session, conversation)` binding that does not
exist. All three were rewritten on 2026-09-26 to claim only what is true — same code path, same
authentication as `PromptAgentConversation`, no weaker route added — and to point here:

- `packages/tddy-service/src/session_agents.rs` — the `IN_JAIL_RELAYABLE` docstring
- `packages/tddy-session-agents/src/lib.rs` — the closed-set test's docstring
- `docs/dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure.md` — Technical Debt

## What would close it

1. Decide whether `session_dirs` should reject a `session_id` the token does not own, or whether
   ownership belongs one layer up. (This determines whether the fix is one port or six handlers.)
2. Give `OpenAgentConversations` a session-scoped lookup — `routing_for(session_id,
   conversation_id)` returning `None` on mismatch — built on the existing `is_with`.
3. Cover it with a test that opens a conversation on session A and drives every one of the six
   `IN_JAIL_RELAYABLE` operations against it with session B's token, asserting each is refused.
