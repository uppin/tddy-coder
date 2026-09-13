# 2026-09-09 — The relay allowlist becomes data

`runner.rs` held five `(service, method)` string literals naming what an in-jail agent may relay to
its host — `StreamSessionAgents`, `OpenAgentConversation`, `PromptAgentConversation`,
`CancelAgentConversation`, `ReportAgentConversationState`. All five are family B, which moved to
`session_agents.SessionAgentService`.

`FORWARDED_RPCS` now reads `tddy_service::session_agents::IN_JAIL_RELAYABLE`, so the allowlist and
the served coordinate cannot drift. **The permitted operation set is byte-for-byte unchanged**; only
the service name each tuple carries. Widening or narrowing it inside a mechanical move is exactly
what a stack like this makes easy and must not do.

The constant lives in `tddy-service` rather than in `tddy-session-agents` precisely so this binary —
the one that runs inside every jail — does not inherit that crate's `livekit` dependency tree.

Why this mattered more than a rename: a mismatched allowlist fails **closed**, silently, at runtime.
Every in-jail subagent conversation would have stopped with no error worth reading anywhere.

`the_sandbox_relay_allowlist_names_the_new_service` was **deleted**, not kept: its `||` was satisfied
by two *comments* in this file, so deleting `FORWARDED_RPCS` entirely left it green, and its first
clause failed on correct code that spells the tuple literally. What replaced it tests the
*enforcement* half — a non-allowlisted rpc is refused `not_found` without a frame reaching the host,
paired with the positive case so the refusal cannot pass vacuously.

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
