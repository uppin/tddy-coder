# 2026-11-06 — Telegram gets the late-bound session starter and the host's registry

**Type:** Feature

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- `Registries::session_starter` (a `LateBoundSessionStarter`) goes to Telegram in `telegram_wiring.rs` with the host's coding-agent registry (`telegram_coding_agents`); `install_session_host` binds `HostSessionStarter` into it. Telegram's `with_terminal_exposure` call is gone (approved, 2026-11-06).
- `tests/session_agent_remote_acceptance.rs` installs its cursor stub through `cursor.yaml`'s `binary.env` on a fixture host, so it no longer needs a real `agent` on `PATH`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).
