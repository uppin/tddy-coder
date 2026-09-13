# 2026-09-09 — The roster client dials the roster's own service

`roster/conversation.rs`, `roster/registry.rs`, `roster/stream.rs` and `subagent_runtime.rs` named
`connection.ConnectionService` for the roster stream and the three conversation RPCs. Family B moved
to `session_agents.SessionAgentService`, and these call sites now read
`tddy_service::session_agents::SESSION_AGENT_SERVICE` rather than spelling the name — the same rule
the sandbox relay allowlist follows, for the same reason.

Nothing about the runtime changed. `SubagentSession`, `LiveAgentRoster` and the `RosterCurrency`
distinction between a roster that never arrived and one that went stale are node 5's and are consumed
unchanged; `tddy-session-agents` serves family B in front of them.

Worth recording for a reader of `tddy-session-agents`: `LiveAgentRoster` is the roster as a **client**
sees it — seeded from `TDDY_SUBAGENTS_JSON` and replaced wholesale by every published frame. Node 7's
draft contract proposed handing it to the service as its state, which would have had
`AttachSessionAgent` write into a copy nobody persists. The authoritative store is
`SessionAgentRosterStore`, in the serving crate; this one is a subscriber.

Docs: [roster-and-subagent-runtime.md](../roster-and-subagent-runtime.md).
Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
