# 2026-11-06 — The argv builders are kept only for Telegram

**Type:** Refactor

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- `CliSessionManager::{start, start_cursor}` and the argv builders they use carry `TODO(cli-dispatch-collapse)`: Telegram (n18, `feature/agent-tools/telegram-spec-launch`) is their last caller, and no daemon path launches through them.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).
