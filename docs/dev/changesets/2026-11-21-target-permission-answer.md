# 2026-11-21 — A target agent's permission prompt is answered by its orchestrator

**Type:** Feature · **Stack:** `#target-parity` 1/7 (PR #969) · **Packages:** `tddy-agent-backend`,
`tddy-agent-launch`, `tddy-session-activity`, `tddy-tools`, `tddy-session-lifecycle`,
`tddy-session-driver`, `tddy-coding-agent-specs` · Feature:
[claude-cli-session.md § On a target session](../../ft/daemon/claude-cli-session.md#on-a-target-session) ·
Backlog: `2026-11-16-coder-hosts-never-run-against-a-real-deployment` (narrowed: the follower's
delivery on a live workspace is evidenced; the answer channel is not),
`2026-11-19-coder-remote-agent-local-paths` (touched, left open: the channel names no host path)

## Problem

On a target every hook was a `printf` appending to `.tddy/hooks.log`, and nothing travelled back, so
`PermissionRequest` was not installed there. Claude asked in its terminal; the daemon saw only the
`Notification` that followed, which names no tool, so `ReadSessionActivity` showed no pending question
and an orchestrator could not answer (probe on `mantasi/remote-dev`, 2026-11-21).

## What changed

- **`tddy-agent-backend`**: `remote_answering_hook_command` and `REMOTE_ANSWERS_DIR` (`.tddy/answers`).
  The hook mints a nonce, appends `<event>@<nonce>\t<token>\t<payload>`, polls `<answers>/<nonce>` for
  `answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS` and prints it; an empty file or none ends it silently.
  `build_claude_hooks_settings` installs `PermissionRequest` as that command on a target, with Claude's
  `timeout` at `answer_wait_secs + CLAUDE_HOOK_TIMEOUT_MARGIN_SECS`. The question matcher stays out.
- **`tddy-agent-launch`**: `HookLine::answer_nonce`; `HookReportDelivery::report_session_status` returns
  the `ReportSessionStatusResponse` and the port gains `await_question_answer`;
  `ActivityHookSink::answering_through(shell, worktree)` spawns `remote_hook_answer::hold_and_answer`
  per waiting line, which holds through the port and writes the decision (or an empty file) with a
  temporary file and `mv`. `.tddy/answers/` joins `info/exclude` and is removed by
  `remove_remote_hook_log`. The target harness launch answers through the session's shell
  (`follow_the_hook_log`, extracted from `launch`).
- **`tddy-session-activity`**: `hook_decision::permission_decision_json`, moved from `tddy-tools`, which
  re-exports it.
- **`tddy-session-lifecycle`**: `ActivityHookDelivery` returns the response and serves
  `await_question_answer` through the activity service.
- **`tddy-session-driver`**: its delivery answers `NO_ORCHESTRATOR`, and its sink answers through the
  session's shell, so a driven agent's permission hook ends at once instead of after its whole wait.

## Validation

- Tests: `remote_hooks_acceptance` (answer printed, denial with message, nobody answers, answers removed
  at the end), `remote_hooks_render_acceptance::claude_code_on_a_target_waits_for_a_permission_answer_in_its_worktree`,
  four unit tests of the waiting command under real `sh`, three of `hook_decision`,
  `a_waiting_hooks_line_carries_its_nonce`, and
  `session_driver_hooks_acceptance::a_driven_session_s_permission_prompt_is_left_to_the_agent_s_terminal_at_once`.
- File length: `remote_hook_follower.rs` 444 → 511 production lines — not split here because n4 (#972)
  and n6 (#974) of the stack modify it; recorded as
  `tddy-agent-launch/docs/code-issues/oversized-file-remote-hook-follower.md` for the stack's sweep.
- `harness_launch.rs::launch` 137 → 119 lines (the follower start extracted).

## Decisions

- The hold is daemon-side, because the `request_id` is minted there and `AwaitQuestionAnswer`
  requires it.
- One node, not "visible" then "answerable": a visible-only step would ship an
  `AnswerSessionPermission` that succeeds while delivering nothing.
- The answer is written atomically (temporary file + `mv`) because `RemoteShell::write_file` is
  `mkdir -p && cat >` and a poller could read half a file.
- Line format of a waiting hook: `<event>@<nonce>\t<token>\t<payload>`; report-only lines are unchanged.
