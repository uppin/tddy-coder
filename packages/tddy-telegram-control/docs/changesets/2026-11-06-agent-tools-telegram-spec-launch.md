# 2026-11-06 — Telegram starts CLI agents through the daemon's generic start

**Type:** Feature

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- `/start-agent <prompt>` (agent keyboard, `tsa:<agent_idx>|s:<uuid>`) offers the host registry's PTY agents; `/start-claude` / `/start-cursor` pre-pick theirs. New module `telegram_session_control/agent_choices.rs`.
- Models come from the chosen spec (`models.default` + `models.curated`, `tcm:` by index); `requires_model` with none is refused; the `feature/<slug>` branch name only for `branch_name_required`.
- The start goes through the `SessionStarter` port (`with_session_starter`, 600 s); the registry through `with_coding_agents`. The pending session is replaced by the started one; a held branch answers with the `tbc:` keyboard from the start's own guard.
- Deleted: `spawn_telegram_{claude,cursor}_cli`, the per-agent model callbacks, keyboards and lists (`CLAUDE_CLI_MODELS`, `CURSOR_CLI_MODELS`, `tcur:`), the claude-only conflict pre-check, the per-agent branch-name helpers, `exposure_is_unwired` / `not_exposed`; approved beyond the plan: `persist_changeset_model` and the harness's `terminal_exposure` / `with_terminal_exposure`.
- No `claude_cli` / `cursor_cli` read and no agent-name gate remain in `src/` (`tests/telegram_reads_no_agent_config.rs`).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).
