# 2026-11-17 — Orchestrators watch the sessions they can see

An orchestrator creates a **watch** over sessions of its project on its host — named by
`session_ids`, or every session it started — for six kinds of event (a question, a permission
prompt, a finished turn, a wait for input, an agent's notification, a session ending), with a
required lifetime of at most 24 hours. From then on, every tool answer it receives carries a
`watchEvents` object with what its watches matched since the last answer: each event once, at
most 50 per answer, with how many were lost to the log's window and which watches ended (at
expiry, on cancel, when the orchestrator's own agent ends, or when every named session has
ended). Watches survive a daemon restart. An orchestrator that makes no tool call receives
nothing — waking an idle one is a separate change. See
[Project concept § Watching sessions](../project-concept.md#watching-sessions).
