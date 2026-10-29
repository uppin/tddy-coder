# 2026-10-29 — AnswerSessionPermission and the waiting PermissionRequest hook

**Type:** Feature

`#home-inbox` 6/8 ([#771](https://github.com/uppin/tddy-coder/pull/771)). An orchestrated
claude-cli child's `PermissionRequest` hook waits for its orchestrator's decision. Across:

- `tddy-service` — `ActivityService.AwaitQuestionAnswer`, `AwaitQuestionOutcome`
  (`ANSWERED`, `NO_ORCHESTRATOR`, `TIMED_OUT`); TS regenerated.
- `tddy-session-activity` — `pending_answers::PendingAnswers` (implements `#coagents`'
  `PermissionAnswering`), `orchestrator_presence::OrchestratorPresence`, `answers::answer_permission`
  with `AnswerRefusal` and `permission_request_view`, `service/question_answers.rs`, and
  `ActivityServiceImpl::with_question_answers(AnswerWaitPorts)`.
- `tddy-discovery` — `AnsweredBy::Orchestrator(session_id)`.
- `tddy-daemon-kernel` — `claude_cli.answer_wait_secs` (default 120).
- `tddy-agent-backend` — `HookCommandParams.answer_wait_secs`; the `PermissionRequest` entry runs
  `--await-answer --await-timeout-secs <wait + 5>` with Claude `timeout` `<wait + 10>`
  (`AWAIT_CLIENT_MARGIN_SECS`, `CLAUDE_HOOK_TIMEOUT_MARGIN_SECS`).
- `tddy-tools` — `session-hook --await-answer`, `permission_decision_json`.
- `tddy-tool-engine` — `project_home_answer_tools.rs` (`AnswerSessionPermission`, a target tool,
  `ProjectHomeAnswerPort`); also a conversation spawner's tool.
- `tddy-session-lifecycle` / `tddy-daemon-rpc` — the daemon's one `PendingAnswers`,
  `ServedTerminalPresence`, and the port impl on `DaemonProjectHome`.
- `tddy-agent-launch` — the project-home prompt names the tool.

Decisions:

- **The hook always runs `--await-answer`**; the daemon decides whether anyone is there, because a
  child is tagged with its orchestrator only after its hooks are written.
- **Presence is a served terminal on this host**, not a pid. An orchestrator on another host gets
  no wait (backlog: `2026-10-29-permission-wait-needs-a-same-host-orchestrator`).
- **Check order**: orchestrator, already answered, pending question, kind, waiter and option —
  "already answered" before the pending question because an answer clears it. A race lost at
  delivery is reported as `AlreadyAnswered` when another answer won it.
- **After the hook is woken** the answer stands: failing to append the activity row or to clear the
  pending question is logged, not refused (revised at validation; the child's next status report
  clears the question).
- **Bounded memory**: answered ids are kept for the last 1024 answers; a wait replaced by a re-open
  never closes the new waiter.
- **Options come from the open waiter**, since only it knows whether Claude sent suggestions
  (`allow_always`); the suggestions ride on the await request because `#home-inbox` 3/8's record
  does not carry them.
- **Conversation spawners may answer** (PRD: any session the child names as orchestrator), but
  cannot read the `request_id` — left as a developer decision (backlog:
  `2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request`).

Code issues: `exec_tool/project_home.rs` 779 → 830, `project_home_tools.rs` 719 → 734 and
`tddy-tool-engine/src/lib.rs` 886 → 887 production lines; splits deferred (stack-shared with
`#home-inbox` 7/8 and 8/8, and the parents), each record carries the measurement.
`tddy-tools/src/session_hook.rs` is at 490, under budget.
