# 2026-09-09 — The hook posts to the coordinate that answers

`session-hook` POSTed `ReportSessionStatus` and `ReportAgentActivity` to
`/rpc/connection.ConnectionService/…`. Both methods moved to `activity.ActivityService`. The
relocation migrated the message *types* in this very file and left the URL behind.

**It would have failed invisibly.** The hook swallows every error and exits 0 by contract, and the
command is baked into every Claude and Cursor CLI session's hook configuration — so session status in
`.session.yaml`, the Telegram attention alerts read off it, and every agent-activity row would simply
have stopped, with nothing said anywhere.

The URL now reads `tddy_service::session_activity::ACTIVITY_SERVICE` rather than a literal, so it
cannot drift from the served coordinate again.

**Nothing caught it, and the reason generalises.** The existing hook test asserted exit 0 against an
*unreachable* daemon — which the fail-quiet contract makes true in every state, including the broken
one. `tests/session_hook_cli.rs` (9 tests) serves the real Connect router with
`activity.ActivityService` mounted and nothing else, runs the real binary against it, and asserts on
**what the service received** — never on the exit code. A client whose contract is to fail quietly
cannot be covered by a test that reads its exit code.

Also re-pointed: the roster and conversation clients in `session_agents/`, `mcp_primitives.rs` and
`server.rs`, all reading `SESSION_AGENT_SERVICE` from `tddy-service` rather than spelling it.

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
