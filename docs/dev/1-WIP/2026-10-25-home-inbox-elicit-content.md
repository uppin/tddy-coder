# Changeset: What a claude-cli session is asking

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — seven packages, proto fields on an existing RPC and event, two new hooks, `#start-parity 6/9`'s pending-question record extended with structured content and a keep rule, a Telegram alert that supersedes an existing one
**Stack:** `#home-inbox 3/8` — branch `feature/home-inbox/elicit-content`, base `feature/home-inbox/terminal-read`
**PRD:** [PRD-2026-10-25-elicitation-content.md](../../ft/daemon/1-WIP/PRD-2026-10-25-elicitation-content.md)

## Initial Discovery

[2026-10-25-home-inbox-elicit-content-initial-discovery.md](./2026-10-25-home-inbox-elicit-content-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-08-29-session-notifications-three-follow-ups-left-open.md](../todo/2026-08-29-session-notifications-three-follow-ups-left-open.md) | ⚠ During — claude-cli elicitations get a bus representation here; the presenter path's keyboard/routing fold stays open |
| [2026-10-17-desktop-cursor-hooks-reach-daemon.md](../todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md) | ⚠ During — the new claude-cli hooks use the same `--daemon-socket` as the existing ones |
| [2026-10-24-sandboxed-main-needs-session-role.md](../todo/2026-10-24-sandboxed-main-needs-session-role.md) | — Unrelated (sandboxed claude-cli has no hooks at all; recorded as out of scope) |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-notifier.md` | ⚠ During — the alert goes in a new module |

## Affected Packages

- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `PermissionRequest` + `PreToolUse(AskUserQuestion)` in `build_claude_hooks_settings`
- [ ] [`tddy-changeset`](../../../packages/tddy-changeset/README.md): `PendingQuestion` (from `#start-parity 6/9`) gains the structured fields; `HookEvent` carries `tool_name` and `tool_input`; both events map to `WaitingForInput`
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): `session_status_request` sends the tool and its input for the two events
- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): `ReportSessionStatusRequest.{hook_event_name, tool_name, tool_input_json}`, `ReportSessionStatusResponse.request_id`, `SessionNotificationEvent.question`, `SOURCE_PENDING_QUESTION`
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `hook_reports.rs` records the structured question, keeps it across the `Notification` that follows, and publishes it once
- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): `SessionNotification.question: Option<QuestionNotice>`, source `PendingQuestion`
- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): the question alert

## Responsibility

- **`tddy_changeset::pending_question::PendingQuestion`** is owned by `#start-parity 6/9`. This node
  adds fields to it, all `#[serde(default)]`, so #755's records and readers are unchanged:
  - `request_id: String`;
  - `tool_name: String`;
  - `tool_input_json: String`;
  - `questions: Vec<PendingQuestionItem { header, question, options: Vec<PendingQuestionOption { label, description }>, multi_select }>`;
  - `at_unix_ms: u64`.

  It also adds `PendingQuestion::summary() -> String`.

  The flat fields keep #755's meaning:
  - `kind` is `permission` for a permission prompt. For an `AskUserQuestion` it is `select` or
    `multi_select`, taken from the first question.
  - `header`, `question` and `options` mirror that first question. For a permission, `question` is
    "Run `<tool>`: <input summary>".
  - `allow_other` is true for `AskUserQuestion`.
- **`tddy_changeset::session_activity`.** `HookEvent` gains `tool_name` and `tool_input`.
  `activity_status_from_hook` maps `PermissionRequest`, and `PreToolUse` with
  `tool_name == "AskUserQuestion"`, to `WaitingForInput`.
- **Proto.**
  - `ReportSessionStatusRequest` gains `hook_event_name = 6`, `tool_name = 7` and `tool_input_json = 8`.
  - `ReportSessionStatusResponse` gains `request_id = 1`.
  - `SessionNotificationEvent` gains `question = 12` (`message QuestionNotice`).
  - The source enum gains `SOURCE_PENDING_QUESTION = 5`.
- **`session-hook`.** `session_status_request` (#755's) fills the three new fields for the two events.
  The hook stays fail-quiet, with empty stdout.
- **`hook_reports.rs`** (`report_session_status_impl`, #755's write and clear):
  - A report carrying `tool_name` records the structured question under a new `request_id`. It
    returns that id and sets `.session.yaml.pending_elicitation`.
  - On the `Notification` that follows, #755 writes a message-only `WaitingForInput` record. That write
    now **keeps** a structured record and only fills an empty `question`.
  - Every other status clears both, as #755 already does.
- **Kernel `QuestionNotice { request_id, kind, tool_name, summary, questions }`.** This is a mirror,
  because `tddy-daemon-kernel` does not depend on `tddy-changeset`.
  - It is built by `From<&PendingQuestion>` in `tddy-session-activity`.
  - `SessionNotification.question: Option<QuestionNotice>`, with source
    `SessionNotificationSource::PendingQuestion`.
  - One notification is published per `request_id`.
- **Hooks.** 8 events, including `PreToolUse` with the matcher `AskUserQuestion`.
- **`tddy_telegram_control::question_alert`.** One alert per `request_id`. `WaitingForInput`'s alert
  is skipped while a structured question is pending.

## Boundaries

- No waiting and no decision output (`6/8`, `8/8`). The hooks return at once.
- No answering tool, and no change to Claude's own prompt.
- No new RPC and no second store. The record is #755's `pending-question.json`.
- No change to #755's `ReadSessionActivity`, its paging, or its `tool`-session recorder.
  `ReadSessionActivity` shows the new fields only because it serialises `PendingQuestion`.
- No sandboxed or cursor-cli capture.
- The web drawer is unchanged beyond `pending_elicitation` now being real (it already reads it).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#start-parity 6/9` session-follow (#755) | `PendingQuestion` and `pending-question.json` (write, clear, read); `ReportSessionStatusRequest.message`; `session_status_request`; the `WaitingForInput` write and clear in `hook_reports.rs`; `ReadSessionActivity` returning the pending question | extends the struct and the request; adds the structured branch and the keep rule to the write; the orchestrator reads the richer record through `ReadSessionActivity`, with no code of its own here | change the file name, the paging, the `tool`-session recorder, or `ReadSessionActivity` |
| `#home-inbox 1/8` agent-notify | `SessionNotification` with optional structured payloads, the `SessionNotificationSource` extension pattern, `TelegramSender::send_html_message` and the HTML renderer | adds the `question` field beside `agent`; sends the alert as HTML | change `AgentNotification`, the renderer, the limits, or the notify RPC |

**Replan 2026-10-26.** This replaces the 2026-10-25 plan's own `Elicitation` record,
`elicitations.json` and `ReportElicitation` RPC. #755 landed the same fact first, and two stores
written by different hooks would disagree.

## Draft PR contract

The first push publishes, with bodies `// TODO(elicit-content): implement`:

- the `PendingQuestion` fields and the `summary` signature;
- the proto fields, regenerated;
- the kernel `QuestionNotice`;
- the `HookEvent` fields;
- the two hook entries;
- the failing tests.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no.
- The Telegram alert test sends through `1/8`'s HTML sender and renderer (behaviour).
- The capture extends #755's write, which sits below in the line.

**Concurrent with:** `#home-inbox 4/8` desktop-notify, `5/8` terminal-keys
**Blocks:** `6/8` permission-answer (request ids and records), `7/8` home-wait (question events)

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A (on #758)

- Six hooks are installed.
- `Notification` maps to `WaitingForInput` with the hook's `message`. #755 stores that as
  `{kind: permission, question: <message>}` in `pending-question.json` and clears it on any other status.
- There is no `PermissionRequest` or `PreToolUse` hook. The tool, its input, and an `AskUserQuestion`'s
  options never reach the daemon.
- `pending_elicitation` is never written for claude-cli.
- Telegram says "needs your input (permission, question, or your next prompt)".

## State B

1. A permission prompt fires `PermissionRequest`.
2. `session-hook` sends `ReportSessionStatus { WaitingForInput, tool_name, tool_input_json }`.
3. The result:
   - `pending-question.json` holds `{request_id, kind: permission, tool_name, tool_input_json, question: "Run Bash: cargo publish"}`;
   - `pending_elicitation` is true;
   - one notification is published, carrying the `QuestionNotice`;
   - one Telegram alert names the tool;
   - `ReadSessionActivity` shows the same record.

An `AskUserQuestion` call does the same, as `select` or `multi_select` with every question. The
`Notification` that follows keeps the record. The next `PostToolUse`, `UserPromptSubmit`, `Stop` or
`SessionEnd` clears it.

## Delta

### New
- The kernel `QuestionNotice`; `question_alert.rs` (telegram-control).

### Modified
- `tddy-changeset`: `pending_question.rs` (fields, `summary`); `session_activity.rs` (`HookEvent`, the mapping).
- `session_hook.rs::session_status_request`.
- `hook_reports.rs`.
- `build_claude_hooks_settings`: 6 → 8 events, with its count test.
- `activity.proto`, regenerated.
- `TelegramDaemonHooks`: admits `PendingQuestion`; skips `WaitingForInput` while a structured question is pending.
- At wrap: `docs/ft/daemon/{claude-cli-session.md,session-notifications.md,telegram-notifications.md}`.

## Testing Strategy

### Acceptance tests

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/pending_question_capture_acceptance.rs`
- [ ] `a_permission_request_records_the_tool_and_its_input_as_the_pending_question_and_publishes_it_once`
- [ ] `an_ask_user_question_pre_tool_use_records_every_question_and_option`
- [ ] `the_notification_that_follows_keeps_the_structured_question`
- [ ] `post_tool_use_user_prompt_submit_stop_and_session_end_each_clear_it_and_reset_pending_elicitation`
- [ ] `a_wrong_hook_token_is_refused_and_records_nothing`

**`tddy-tools`**: `packages/tddy-tools/tests/session_hook_pending_question.rs` (#755's suite)
- [ ] `the_permission_and_question_hooks_send_the_tool_and_input_and_exit_zero_with_empty_stdout`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/session_follow_acceptance.rs` (#755's suite)
- [ ] `read_session_activity_shows_a_claude_cli_permissions_tool_and_input`

**`tddy-telegram-control`**: `packages/tddy-telegram-control/tests/question_alert_acceptance.rs`
- [ ] `a_permission_alert_names_the_tool_and_its_input_summary_once_per_request`
- [ ] `a_question_alert_lists_the_question_and_its_options`
- [ ] `no_needs_your_input_alert_follows_while_the_same_prompt_is_pending`

**`tddy-agent-backend`**: tests in `packages/tddy-agent-backend/src/claude_hooks.rs`
- [ ] `build_claude_hooks_settings_emits_all_eight_events` (replaces the six-event test)
- [ ] `the_pre_tool_use_hook_matches_ask_user_question_only`

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Capture, keep and clear | Integration over the activity service with a temp sessions dir | The persisted record and the flag are the behaviour |
| Hook contract | Process-level `session-hook` against a fake service | Exit code and stdout are Claude's contract |

## Decisions & Trade-offs

- **Extend #755's record rather than keep a second one** (replan 2026-10-26).
  - The id is `request_id`, matching `#coagents`' `PermissionRequestView.request_id`.
  - The stack retires `Elicitation`, `ElicitationStore`, `ReportElicitation` and `elicitation_id`.
- **No new RPC.** The two hooks report `WaitingForInput`, which is what they are.
- **No history of resolved questions here.** `7/8`'s log is the history.
- **Report-only hooks here.** Waiting is opt-in per hook, in `6/8` and `8/8`.
- **The question alert supersedes `WaitingForInput`'s alert** for the same prompt, so the operator is
  told once.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-elicit-content-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
