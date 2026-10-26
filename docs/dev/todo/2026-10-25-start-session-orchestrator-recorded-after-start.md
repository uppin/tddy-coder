# 2026-10-25 — StartSession's orchestrator is written after the start returns

**Category:** Deferred from `#start-parity` 2/9 (#751)
**Source:** `packages/tddy-agent-launch/src/svc_start_session_core/orchestrator_record.rs` `record_orchestrator` (`TODO(start-parity)`)

The daemon creating a session records `orchestrator_session_id` / `orchestrator_daemon_instance_id`
by reading the new session's changeset back and rewriting it once the start has returned — after
the session's agent is already running. A reader that looks in the window between the two writes sees
no orchestrator, and a write by the agent in that window can race the rewrite.

**Why deferred:** threading the fields into each session type's first changeset write
(`write_initial_changeset`, the workspace start) touches every start branch; #751 kept to one place.

**What would close it:** carry the orchestrator into the initial changeset each start path writes and
delete `record_orchestrator`.
