# 2026-10-25 — An orchestrated `tool` session start is refused

**Category:** Deferred from `#start-parity` 2/9 (#751) — open developer question
**Source:** `packages/tddy-agent-launch/src/start_request_checks.rs` `refuse_an_orchestrated_tool_start` (`TODO(start-parity)`)

`StartSession` from a project-home session with `session_type: "tool"` (or empty) is refused with
`FAILED_PRECONDITION`. A `tool` session's changeset is written by its own `tddy-coder` process, not
by the daemon, so the daemon cannot record `orchestrator_session_id` in it the way it does for every
other session type.

**Why deferred:** recording it needs a new `tddy-coder` argument and a write in its changeset
bootstrap — a change to the tool session's spawn contract that #751 did not plan. Whether an
orchestrator should start `tool` sessions at all is still an open question for the developer.

**What would close it:** pass the orchestrator (session and host) to the spawned `tddy-coder`, which
records it as it records `--stack-parent`; drop the refusal and add a `tool` case to
`start_session_tool_acceptance.rs`.
