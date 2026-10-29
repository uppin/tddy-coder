# 2026-10-26 — A session with no shell cannot notify its operator

**Category:** Deferred from `#home-inbox` 1/8 (#766)
**Source:** `docs/dev/changesets/2026-10-26-home-inbox-agent-notify.md`;
`packages/tddy-agent-launch/src/claude_cli_spawn.rs` and `svc_resume_claude_cli_session.rs` (the
`role.name` / `session_role` gate before the notify wiring)

#766 lets an agent notify its operator in its own words by running
`tddy-tools notify "<title>" --body "<markdown>"`, with its credentials (`TDDY_SESSION_ID`,
`TDDY_OS_USER`, `TDDY_HOOK_TOKEN`, `TDDY_DAEMON_URL` | `TDDY_DAEMON_SOCKET`) exported into its
process environment at start and resume. That needs a shell.

A session with a **standing role** — the project-home main (`session_role = project-home`) — is
launched with no shell tool, so it cannot run the CLI. The launch therefore does not give it the
notify environment or the guidance paragraph at all: a start or resume whose role name is non-empty
skips the wiring. The home main is exactly the agent that most often has news for an absent
operator (its children finished, one is blocked).

Sandboxed claude-cli sessions are excluded for a different reason: they have no hooks and so no hook
token for `ReportOperatorNotification` to check.

**Why deferred:** #766's Boundaries exclude an MCP tool ("No MCP `NotifyOperator` tool; the home main
cannot notify"). The developer chose the CLI-plus-environment model for this node (2026-10-25).

## What would close it

An MCP `NotifyOperator { title, body_markdown, level, sound }` tool served to a session with a
standing role, calling `ActivityService.ReportOperatorNotification` on the session's own daemon with
the hook token its `.session.yaml` records (or a role-scoped credential the daemon checks instead),
and the guidance paragraph rewritten to name the tool rather than the CLI for those sessions. Test: a
project-home main's `NotifyOperator` publishes one `AGENT` attention notification on the bus, and a
session without the role is not offered the tool.
