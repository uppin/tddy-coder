# 2026-10-29 — A child's permission hook waits only for an orchestrator on its own host

**Category:** Deferred from `#home-inbox` 6/8 (#771)
**Source:** `packages/tddy-session-activity/src/service/question_answers.rs` (`orchestrator_is_present`)

`AwaitQuestionAnswer` holds a claude-cli child's `PermissionRequest` hook only when the child's
changeset `orchestrator_session_id` names a session **this** daemon serves the main terminal of. A
child whose changeset records an `orchestrator_daemon_instance_id` of another host gets
`NO_ORCHESTRATOR` at once, and Claude shows its own prompt — even though `AnswerSessionPermission`
is a target tool and would be forwarded to the child's host.

**Why deferred:** whether a session on a peer daemon is live is a new peer RPC (or a field on an
existing peer listing), outside #771's boundaries.

## What would close it

A peer query "does daemon X serve session Y's main terminal", asked by `ServedTerminalPresence`
(`packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs`) when the changeset
names another host, with `orchestrator_is_present` no longer refusing other hosts outright. Test: a
child on host A whose orchestrator runs on host B waits, and B's forwarded `AnswerSessionPermission`
wakes it.
