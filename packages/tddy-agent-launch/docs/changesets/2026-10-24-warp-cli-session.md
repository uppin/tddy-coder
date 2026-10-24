# 2026-10-24 — Resume-token recorder and token-carrying CLI-harness resume

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md). Feature: [Warp CLI session](../../../../docs/ft/daemon/warp-cli-session.md).

- `cli_resume_token.rs`: `resume_token_recorder(session_dir) -> (ExitHook, ResumeTokenArm)` — the exit
  hook writes `harness_resume_token` (and nothing else) when the capture holds a token, once armed after
  the start or resume has written the whole metadata; a dropped arm records nothing.
  `settled_resume_token` waits on `CliSessionManager::exit_hook_settled`, then re-reads the token.
- `opencode_cli_spawn.rs`: the recorder is handed to `launch_cli_harness` at start and resume; the
  generic resume passes the recorded token as `CliLaunch.conversation_id`.
- `cli_harness_jail::require_sandbox_hosts`, called in `cli_branch_starts.rs`: a sandboxed start of a
  harness the runner does not host is `FAILED_PRECONDITION` "the sandbox runner does not host <type>"
  before the provider route is resolved.
- `route_on_account` checks `harness_routable` before the empty-model refusal, so a `warp-cli` start
  naming an account is refused naming Warp.
- `harness_profile_start.rs`: the `Harness::WarpCli` arm. No new arm in `start_session_on_request` or
  `resume_session_at_session_coordinate`.

Tests: `tddy-session-lifecycle/tests/warp_cli_session.rs` (14).
