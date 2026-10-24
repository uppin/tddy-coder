# 2026-10-24 — Warp terminal sessions that resume their conversation

**Type:** Feature

`#warp-agent` 1/2 ([#717](https://github.com/uppin/tddy-coder/pull/717)); the successor is
[#718](https://github.com/uppin/tddy-coder/pull/718) (agent terminal tabs). A fifth CLI harness,
`warp-cli`, through the CLI harness registry, across:

- `tddy-agent-backend` — the `WARP_CLI` descriptor in `cli_harness_warp.rs` (argv `warp` /
  `warp --resume <token>`, an empty curated catalogue), `resume_token_from_output` (the exit-line reader,
  escape sequences stripped), `Harness::WarpCli` refused on every preset as Cursor is, and the `warp`
  context-glob row. `context_globs_for_agent` and `SHARED_CONTEXT_GLOBS` moved into
  `backend/context_globs.rs`, re-exported at the same path.
- `tddy-agent-launch` — `cli_resume_token.rs`: the resume-token recorder (an exit hook, armed once the
  start or resume has written the metadata) and `settled_resume_token`; the generic CLI-harness resume
  passes the recorded token as `CliLaunch.conversation_id`; `route_on_account` checks `harness_routable`
  before its empty-model refusal; a sandboxed start of a harness the runner does not host is refused by
  `require_sandbox_hosts` before the route is resolved.
- `tddy-cli-sessions` — `HarnessLaunchExtras { args, env, on_exit }` on `launch_cli_harness`, `ExitHook`
  run by the terminal-cleanup task before the terminal leaves the index, and `exit_hook_settled`.
- `tddy-pty` — the exit monitor waits for the PTY reader to drain (bounded by `READER_DRAIN_GRACE`, 1 s)
  before reporting exit, and the PTY control is registered before the ready signal.
- `tddy-changeset` — `SessionMetadata.harness_resume_token` (mechanical `None` literals in
  tddy-session-split, tddy-session-files, tddy-telegram-control, tddy-testing-commons, tddy-daemon-rpc,
  tddy-daemon and the crates' tests; no behaviour change there).
- `tddy-session-activity` — `warp-cli` worktrees removed on delete.
- `tddy-web` — the Warp toggle entry, no model or provider field, Create enabled with only a project,
  `model: ""` in the request.

Tests: `tddy-agent-backend/tests/warp_cli_acceptance.rs`, `tddy-session-lifecycle/tests/warp_cli_session.rs`
(14 daemon-level tests against a stub `warp`, including
`resuming_a_warp_session_after_it_exits_continues_the_conversation_it_printed`),
`tddy-cli-sessions/tests/exit_hook.rs`, the `tddy-pty` runtime unit tests
`output_written_just_after_the_child_exits_is_captured_when_the_exit_is_reported` (Linux only) and
`a_process_left_holding_the_terminal_does_not_hold_back_the_exit`,
`tddy-session-activity/tests/worktree_removal_eligibility.rs`, and
`tddy-web/cypress/component/CreateSessionWarpAcceptance.cy.tsx`.

Code issue `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md`: 906 → 908
production lines with the `warp` context-glob row, then 908 → 850 by moving `context_globs_for_agent` into
`backend/context_globs.rs`. **Partially fixed**, still open (850 against a 500 budget; seams A–D remain).
The start and resume dispatch got no new arm (`start_session_on_request`,
`resume_session_at_session_coordinate`): `warp-cli` reaches the generic path through the registry lookup.

New backlog entries: `2026-10-24-warp-cli-facts-unconfirmed` (Warp's argv and exit banner are
unconfirmed), `2026-10-24-warp-cli-sandbox` (a `warp-cli` session cannot run in the sandbox),
`2026-10-24-warp-provider-account` (Warp runs only on the person's own Warp sign-in) and
`2026-10-24-warp-auto-approve` (no `--auto-approve` option for Warp sessions). No backlog entry was
resolved; `2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau` stays open
(cursor-cli still leaks; `warp-cli` does not join it).

Feature: [Warp CLI session](../../ft/daemon/warp-cli-session.md).
