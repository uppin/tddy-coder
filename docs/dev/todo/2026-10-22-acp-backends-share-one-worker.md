# 2026-10-22 — The three ACP backends each carry their own copy of the worker

**Category:** Deferred from `#provider-route` 7/9 (#668) — `TODO(opencode-acp)` in
`packages/tddy-agent-backend/src/backend/opencode_acp.rs`
**Source:** `docs/dev/1-WIP/2026-10-18-provider-route-opencode-acp.md` § Decisions & Trade-offs

`ClaudeAcpBackend` (`backend/acp.rs`), `CodexAcpBackend` (`backend/codex_acp.rs`) and
`OpenCodeAcpBackend` (`backend/opencode_acp.rs`) each run their agent through their own worker
thread: spawn the child with its route, connect `ClientSideConnection` over its stdio, `initialize`
once, `new_session` or `load_session`, `prompt`, and answer `ListModels` from a new session's models.
The three loops differ only in what surrounds them — Codex retries through `codex login` on an
auth-like error, Claude ACP carries questions and the `Shutdown` command, OpenCode writes the route's
configuration file.

#668 shared the part that was identical in all three: the `session/update` handling and the
permission answer (`backend/acp_updates.rs`, `record_session_update` and `allow_once`).

**Why deferred:** sharing the worker changes the two existing backends' control flow — Codex's OAuth
retry wraps `new_session` / `load_session`, and both carry measured complexity records
(`complexity-acp-run-acp-worker`, `complexity-codex-acp-run-codex-acp-worker`) that a shared worker
would close or move. That is a restructure of code #668 does not otherwise own, on files no
`#provider-route` node changes, and it is reviewable on its own.

**What would close it:** one generic ACP worker in `tddy-agent-backend` parameterised by a small
trait (how to spawn the child; what to do on a failed session call), with the three backends as
implementations; the two complexity records re-measured and closed; `TODO(opencode-acp)` removed.
