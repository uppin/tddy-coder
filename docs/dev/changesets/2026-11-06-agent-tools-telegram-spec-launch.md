# 2026-11-06 — Telegram starts agent sessions through the spec launch, with registry agents and spec models

**Type:** Feature (Telegram sessions gain hooks, routes, tmux and activity status), with planned and approved deletions

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)), branch
`feature/agent-tools/telegram-spec-launch`. Based on `feature/agent-tools/cli-dispatch-collapse`
([#823](https://github.com/uppin/tddy-coder/pull/823)). Blocks
`feature/agent-tools/agent-config-removal` (n19, #825): no `claude_cli:` / `cursor_cli:` reader is
left in Telegram.

Packages: `tddy-telegram-control`, `tddy-daemon-extensions`, `tddy-agent-launch`,
`tddy-session-lifecycle`, `tddy-daemon`, `tddy-cli-sessions`.

## What changed

A Telegram CLI start is the daemon's one generic start. A Telegram-started agent session is
indistinguishable from a web-started one: the spec launch renders its argv, hooks file and hook
token, MCP, provider route, tmux, git identity and metadata, and the start's own branch-conflict
guard answers for the branch.

- **Session-starter port** (`tddy-daemon-extensions::session_starter`): `SessionStarter::start_for_operator(OperatorSessionStart)`
  → `Started { session_id }` or `BranchConflict(..)`; `NoSessionStarter` refuses
  `FAILED_PRECONDITION`; `LateBoundSessionStarter` is bound once, after the host exists, and refuses
  a start before the bind instead of waiting.
- **Os-user entry** (`tddy-agent-launch/src/operator_start.rs`):
  `LaunchSessions::start_session_for_os_user` is `start_session_core` minus the token-to-user step.
  It refuses a request naming a session token or a peer daemon. No token is minted; none reaches the
  agent. `start_here` and `refuse_a_requested_role` became `pub(crate)` for it.
- **Host starter** (`tddy-session-lifecycle::connection_service::operator_session_starter`):
  `HostSessionStarter` maps the start to a `StartSessionRequest` with
  `on_branch_conflict = "reject"` (`operator_start_request`) and the response back
  (`operator_start_outcome`).
- **Daemon wiring**: `Registries::session_starter` (one `LateBoundSessionStarter`) goes to Telegram
  in `telegram_wiring.rs` (`with_session_starter`, with the host's registry through
  `with_coding_agents`), and `install_session_host` binds the host into it.
- **Telegram** (`tddy-telegram-control`): `/start-agent <prompt>` offers the registry's PTY agents
  (`tsa:<agent_idx>|s:<uuid>`); `/start-claude` and `/start-cursor` pre-pick their agent. The model
  keyboard is the spec's `models.default` + `models.curated` (`tcm:`), labelled by id; an agent with
  `requires_model` and no models is refused, one with neither starts without a model. The new-branch
  name is `feature/<slug>` only for `branch_name_required`. A host with no coding agents, or a
  shortcut naming an agent it lacks, is refused in the chat before anything is created. After a
  start the pending session directory is removed and the chat follows the started id; on a refusal
  or a conflict the pending session stays. Telegram bounds the start at 600 s.

## Removed

Planned (changeset § Removed): `spawn_telegram_claude_cli`, `spawn_telegram_cursor_cli`,
`handle_telegram_{claude,cursor}_model_callback`, `send_{claude,cursor}_model_pick_keyboard`,
`exposure_is_unwired`, `not_exposed` (and their two unit tests), Telegram's `CLAUDE_CLI_MODELS` /
`CURSOR_CLI_MODELS`, the `tcm:`/`tcur:` per-agent constants and parsers (`tcm:` lives on as the
generic `CB_TELEGRAM_MODEL`), Telegram's claude-only branch-conflict pre-check,
`claude_cli_branch_name_from_changeset`, `cursor_cli_branch_name_from_session_id`,
`is_claude_cli` / `is_cursor_cli`; `tddy-agent-launch` `chat::{install_cursor_hooks_in_worktree,
mint_cursor_chat_id}`; `tddy-cli-sessions` `CliSessionManager::{start, start_cursor}` and
`build_cursor_argv`; the backlog record `2026-11-03-cursor-hand-built-launch-paths.md` (resolved).
`build_claude_argv` and `with_claude_launch_flags` stay — the split agent's `start_with_options` is
their last caller. `tddy_agent_backend::backend::{claude_cli_models, cursor_cli_models}` stay —
`cli_harness` reads them.

**Beyond the plan, approved by the developer (2026-11-06):**

- `persist_changeset_model` (Telegram) — the model is a request field now, no longer a raw
  `changeset.yaml` marker.
- The Telegram harness's `terminal_exposure` field and `with_terminal_exposure` builder, and the
  daemon's call to it in `telegram_wiring.rs` — the session host exposes a Telegram-started session's
  PTY as it does a web one.
- `CliSessionManager::resume_cursor` — no caller left.
- Fourteen test files moved from `CliSessionManager::start` to `start_with_options`: in
  `tddy-cli-sessions` `capture_recorder.rs`, `exposed_terminals.rs`,
  `pty_terminal_service_snapshot.rs`; in `tddy-session-lifecycle`
  `agent_terminal_history_recording_acceptance.rs`, `agent_terminal_tabs_acceptance.rs`,
  `claude_cli_permission_mode_acceptance.rs`, `claude_cli_session_acceptance.rs`,
  `dormant_session_tabs_acceptance.rs`, `support/mod.rs`, `task_service_acceptance.rs`,
  `terminal_control_acceptance.rs`, `terminal_input_ack_acceptance.rs`,
  `terminal_mode_replay_acceptance.rs`, `terminal_session_acceptance.rs`. Alongside them,
  `cli_harness_parity.rs` lost its `build_cursor_argv` parity case,
  `cursor_cli_session_acceptance.rs` its `resume_cursor` cases, and `no_agent_name_gates.rs` its
  note on what Telegram still kept.

## Fixed here

`tddy-daemon/tests/session_agent_remote_acceptance.rs`: daemon A's co-located `cursor-cli` start
resolved `agent` through the generic launch, which never read the fixture's `cursor_cli.binary_path`.
The suite passed only where a real Cursor CLI sat on `PATH` (`~/.local/bin/agent`), and would fail on
CI. The failure predates this node: every file on the resolution path, and the test, is identical at
the base. A's coding agents now resolve against a fixture host whose `TDDY_CURSOR_AGENT`
(`cursor.yaml`'s `binary.env`) names the stub and whose `TDDY_TOOLS_BINARY` names this build's
`tddy-tools`; the dead `cursor_cli:` section left the fixture.

## Tests

New: `tddy-telegram-control/tests/telegram_start_agent_from_spec.rs` (6), `telegram_agent_choices_unit.rs`
(11), `telegram_reads_no_agent_config.rs` (2), `telegram_branch_conflict_acceptance.rs`
`every_registry_backed_picker_callback_fits_in_64_bytes`; `tddy-daemon-extensions` `session_starter`
unit tests (3); `tddy-session-lifecycle/tests/operator_session_starter_unit.rs` (4). Re-pointed:
`telegram_start_claude_acceptance.rs`, `telegram_start_cursor_acceptance.rs`,
`telegram_branch_conflict_acceptance.rs` — fixtures install coding agents (built-in registry + stubs
via `binary.env`) instead of `ClaudeCliConfig` / `CursorCliConfig` literals.

## Code issues

Measured at wrap; restructuring deferred to the sweep node (n20) by the developer.

| Record | Before → after | Note |
|---|---|---|
| `tddy-telegram-control` `oversized-file-telegram-session-control-pickers` | 617 → 627 | regressed |
| `tddy-telegram-control` `oversized-file-telegram-session-control-callbacks` | 736 → 682 | better, still over |
| `tddy-telegram-control` `complexity-…-handle-telegram-branch-callback` | 177 → 135 lines | better, still over 60 |
| `tddy-telegram-control` `complexity-telegram-bot-telegram-callback-handler` | 421 → 415 lines | better, still over |
| `tddy-telegram-control` `crap-telegram-bot-handlers` | touched | not re-derived |
| `tddy-telegram-control` `oversized-file-telegram-bot` | 788 → 798 | **new record**, regressed by this node |
| `tddy-agent-launch` `complexity-svc-start-session-core-start-session-core` | `start_here` 265 → 265 | unchanged (visibility only) |
| `tddy-session-lifecycle` `oversized-file-connection-service` | +3 | the module declaration |

## Backlog

- Deleted: `2026-11-03-cursor-hand-built-launch-paths.md` (resolved).
- Added: `2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md` (for n20);
  `2026-11-06-generic-session-start-has-no-deadline-of-its-own.md` — parent finding:
  `start_session_for_os_user` / `start_session_core` have no overall deadline, so Telegram wraps
  them in 600 s.
- Re-checked, kept: `2026-10-22-telegram-tool-sessions-choose-no-provider-account.md`.
