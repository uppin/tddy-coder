# 2026-10-24 — One ACP client driver for every ACP backend

**Type:** Technical Improvement

`#coagents` 1/8 ([#720](https://github.com/uppin/tddy-coder/pull/720)). New `backend/acp_driver.rs`
(+ `client.rs`, `mapping.rs`, `session.rs`, `worker.rs`; re-exported): `AcpDriver` spawns one agent
child, opens or loads sessions with stdio MCP servers, applies a requested model with
`session/set_model` (refusing one the agent does not advertise, naming the advertised ones), prompts
into `AcpTurnEnd` plus an `AcpUpdate` stream, cancels a running turn, lists models and kills its child
on drop. Permission requests go to an injected `PermissionHandler` (`AllowOnce` for the workflow
backends); per-agent behaviour is an `AcpAgentHooks` (`CodexLoginHooks`, OpenCode's route-file hold).
`ClaudeAcpBackend`, `CodexAcpBackend` and `OpenCodeAcpBackend` run on it; `claude-acp` now resumes by
`session/load` and honours the request's model off a route. `backend/acp_updates.rs` folded into
`acp_driver/mapping.rs`. `tddy-acp-stub`: `TDDY_ACP_STUB_RECORD` recorder, `hold_until_cancel`,
advertised `models`. Tests: `tests/acp_driver_acceptance.rs` (9), `tests/claude_acp_on_driver_acceptance.rs` (2).

Behaviour notes: `AllowOnce` selects the first offered `allow_once` option (no longer the literal id
`allow-once`) and cancels a request with none; a failed spawn is retried on the next request instead
of being kept; a second prompt on a session with a turn running is refused.

Backlog: `docs/dev/todo/2026-10-22-acp-backends-share-one-worker.md` **resolved and deleted**;
`TODO(opencode-acp)` removed.

Code issues — final measurement (structural scan: length / nesting / branch lines / early exits;
the scan reads 1–2 above the records' length and branch counts on the `master` bodies):

| Record | Before (`master`) | After | Verdict |
|---|---|---|---|
| `complexity-acp-run-acp-worker` | 185 / 10 / 18 / 1 (record: 184 / 10 / 18 / 1) | function removed; successors `worker.rs::serve` 45 / 3 / 3 / 2, `dispatch` 24 / 4 / 1 / 0, `supervise` 17 / 4 / 1 / 0, `session.rs::open_session` 58 / 4 / 2 / 0, `prompt` 29 / 2 / 0 / 0 | **closed**, record deleted |
| `complexity-codex-acp-run-codex-acp-worker` | 246 / 13 / 26 / 1 (record: 244 / 13 / 24 / 1) | function removed; successors `CodexLoginHooks::on_session_error` 16 / 3 / 3 / 2, `open_session` 14 / 2 / 1 / 0 | **closed**, record deleted |
| `oversized-file-backend-mod` | 906 production lines | 906 (one line changed) | unchanged, open |

No changed file is over 500 production lines (largest `acp_driver.rs` 412). See
[architecture](../architecture.md).
