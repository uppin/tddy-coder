# 2026-11-09 — A daemon-launched `codex-acp`'s login retry runs as the daemon user and writes into the worktree

**Category:** Found by `acp-subagent-leftovers` (#835, `#agent-tools 27/32`) at `/validate-changes`

**Source:** `docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md`, § Backlog

## What remains

Since #835, `DaemonCodingAgentLauncher::launch_agent` prepares every declared
`coding_agent.acp.hooks_adapter`, so a daemon-launched `codex-acp` subagent gets
`codex-acp-login`'s one `codex login` retry on an auth-like `session/new` error (D4). Two
consequences were accepted for that node and are not fixed:

1. **Wrong OS user.** The hooks run inside the daemon (`CodexLoginHooks::on_session_error`,
   `tddy-agent-backend/src/adapters/codex_acp_login.rs`), so `codex login` signs in **the daemon's
   user**, while the agent runs as the session's OS user (`setpriv` or supervisor relay). For a
   session whose OS user is not the daemon's, the retry signs in the wrong account and the retried
   `session/new` fails again.
2. **Authorize-URL file in the worktree.** `CodingAgentSession::opened`
   (`tddy-coding-subagent/src/session.rs`) hands `begin_open` the open's `cwd` — the worktree — as
   the session directory, so the login writes `codex_oauth_authorize.url` there, where an agent
   commit can pick it up.

## What would close it

Run the login as the session's OS user (the same `run_as_session_user` / supervisor relay path the
agent itself takes), and hand `begin_open` a per-conversation directory outside the worktree,
owned by that user. Pin both with an acceptance test in
`tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` over a stub `codex` that records
its uid and output directory.

## Why it stays

Fixing it needs the login spawned through the user switch, a session-user file ownership problem.
The OpenCode route-file record that shared it was closed by `#agent-tools` 29/32 (#857) without
solving it — OpenCode's route went inline, so no file needed an owner — and this one now stands
alone.
