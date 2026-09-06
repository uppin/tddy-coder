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
