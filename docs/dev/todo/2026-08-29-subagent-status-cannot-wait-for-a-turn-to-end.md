# 2026-08-29 — `subagent_status` cannot wait for a turn to end

**Category:** Future enhancement
**Source:** subagent-status-wait changeset, 2026-08-29

`subagent_status { waitFor: "ready" }` covers the case it was asked for — parking until an agent
whose checkout is still provisioning becomes promptable. The symmetric want is `waitFor: "idle"`:
park until a **running** agent's turn ends, so a main agent that fired a prompt and went off to do
something else can rejoin without polling. The predicate is one line and the wait machinery is
already there; it is out of scope only because nothing asks for it yet.

The one design question it raises and `"ready"` does not: `unknown` (`SESSION_AGENT_STATUS_UNSPECIFIED`)
counts as *ready* deliberately — a restored roster has nothing to say about an agent that is
nonetheless promptable — but it must **not** count as *idle*. A wait treating "nothing to say" as
"the turn finished" would report a turn complete that may still be running.

## Re-read at `#unbundle` node 7's wrap (2026-09-12)

Open and unchanged. `#unbundle` node 7 moved the whole conversation path into
`tddy-session-agents` and the changeset made a point of not making the missing wait harder to add:
`subagent_status`'s readiness plumbing keeps its shape, the status vocabulary
(`SessionAgentStatus`, `ManagedAgentState`) is unchanged, and `SESSION_AGENT_STATUS_UNSPECIFIED`
still maps the way this entry's design question depends on.

What changed is where the predicate would go: `packages/tddy-session-agents/src/session_agent_status.rs`
for the mapping, and `packages/tddy-tools`' `session_agents/` client for the wait itself — the wait
machinery did not move. The `unknown`-is-ready-but-not-idle distinction is now pinned by the unit
tests that travelled with `session_agent_status.rs`.
