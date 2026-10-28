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
- [ ] `a_permission_request_reports_waiting_with_the_tool_and_its_input`
- [ ] `an_ask_user_question_reports_waiting_with_every_question_as_the_tool_input`

**`tddy-tools-cli`**: `packages/tddy-tools-cli/tests/session_hook_cli.rs` (process level; **moved here from `tddy-tools`** — that package has no binary to spawn)
- [ ] `the_permission_request_hook_sends_the_tool_and_input_and_exits_zero_with_empty_stdout`
- [ ] `the_ask_user_question_hook_sends_the_tool_and_input_and_exits_zero_with_empty_stdout`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/session_follow_acceptance.rs` (#755's suite)
- [ ] `read_session_activity_shows_a_claude_cli_permissions_tool_and_input`

**`tddy-telegram-control`**: `packages/tddy-telegram-control/tests/question_alert_acceptance.rs`
- [ ] `a_permission_alert_names_the_tool_and_its_input_summary_once_per_request`
- [ ] `a_question_alert_lists_the_question_and_its_options`
- [ ] `no_needs_your_input_alert_follows_while_the_same_prompt_is_pending`

**`tddy-agent-backend`**: tests in `packages/tddy-agent-backend/src/claude_hooks.rs`
- [ ] `build_claude_hooks_settings_emits_all_eight_events` (replaces the six-event test)
- [ ] `the_pre_tool_use_hook_matches_ask_user_question_only`

### Unit tests (6f)

| Surface | File | Pins |
|---|---|---|
| `PendingQuestion::summary` | `packages/tddy-changeset/tests/pending_question_summary.rs` | Bash → its `command`; Edit/Write/Read → `file_path`; any other tool → its input as compact JSON; a question → its first question's text; a record naming neither → `""` |
| `activity_status_from_hook_with_tool`, `HookEvent` | `packages/tddy-changeset/src/session_activity.rs` (`mod tests`) | `PermissionRequest` and `PreToolUse(AskUserQuestion)` → `WaitingForInput`; `tool_name` and `tool_input` parsed. Guards that already hold: `PreToolUse` of any other tool, and `PostToolUse` with a tool name, are unchanged |
| `question_notice_for` | `packages/tddy-session-activity/tests/pending_question_notice.rs` | field mapping for a permission and for a two-question record |
| `QuestionAlert::render` | `packages/tddy-telegram-control/tests/question_alert_render.rs` | permission and question layouts, no header line, multi-select hint, several questions, HTML escaping (`<`, `>`, `&`; quotes untouched) |

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

### Pinned by the acceptance tests (reviewed 2026-10-26)

- **One notification per question.** A report carrying a question publishes a single
  `SessionNotificationSource::PendingQuestion` notification, and no separate `ActivityStatus` one.
- **The follow-up carries the notice.** The `Notification` hook that follows the same prompt keeps the
  record and publishes an `ActivityStatus` notification whose `question` is the pending notice.
  Telegram declines an `ActivityStatus` notification whose `question` is set, so the operator is told
  once. This is stateless: `TelegramDaemonHooks` keeps no per-session memory.
- **`summary()`.** For a permission it is the input summary (`cargo publish`); the record's `question`
  is `Run <tool>: <summary>`. For an `AskUserQuestion` it is the first question's text.
- **Alert HTML.** Sent loud (`silent: false`), one message per `request_id`. Only `<`, `>` and `&` are
  escaped; quotes are left alone.
  - Permission: `<i>Session {label} wants to run</i> <code>{tool}</code>: <code>{summary}</code>`
  - Question: `<i>Session {label} asks</i>`, then per question a blank line, `<b>{header}</b>` (omitted
    when the header is empty), the question text (with ` (choose several)` appended when
    multi-select), a blank line, and one `• {label}: {description}` line per option (`• {label}` when
    the description is empty). A second question starts after a blank line.
  - Example: `<i>Session my-branch asks</i>\n\n<b>Language</b>\nWhich language?\n\n• Rust: Memory safe\n• Go`

### Deviations from the draft contract (found at 6d, approved 2026-10-26)

1. `ReportSessionStatusResponse.request_id` is field **2**: `ok` already holds 1 on the wire.
2. `From<&PendingQuestion> for QuestionNotice` is the free function
   `tddy_session_activity::pending_question_notice::question_notice_for`: both types are foreign to
   `tddy-session-activity`, so the orphan rule refuses the impl.
3. `HookEvent` loses `Eq` (`tool_input` is a `serde_json::Value`); `PartialEq` stays.
4. The tool-aware mapping is a new `activity_status_from_hook_with_tool`; `activity_status_from_hook`
   keeps its signature and delegates to it.
5. Adding fields forced mechanical edits to existing literals:
   - `..Default::default()` on `PendingQuestion` in `tddy-coder` (`src/session_participant/pending_question.rs`,
     `tests/pending_question_from_mode.rs`) and `tddy-changeset/tests/pending_question.rs` (#755's);
   - `..Default::default()` on `ReportSessionStatusRequest` in tests of `tddy-session-lifecycle`,
     `tddy-daemon` and `tddy-telegram-control`;
   - `question: None` on about 14 `SessionNotification` literals across 7 files, **including #766's
     test files** — #766's green rebase may conflict there.
6. `each_event_has_one_matcher_group_with_one_command_hook` no longer asserts `matcher == ""` for every
   event, because `PreToolUse` carries `AskUserQuestion`. The matchers are pinned whole by
   `the_pre_tool_use_hook_matches_ask_user_question_only`.
7. The process-level hook test moved from `tddy-tools` to `tddy-tools-cli/tests/session_hook_cli.rs`,
   and is two tests (one per hook).

## Risks for green

- **A stale pending call id.** `session-hook` also sends `ReportAgentActivity` for every `PreToolUse`.
  Until now no claude-cli session had a `PreToolUse` hook; with this node's `AskUserQuestion` one,
  `report_agent_activity_impl` pushes a pending `call_id` that only a `PostToolUse` pops. If the
  operator denies the question, no `PostToolUse` follows, and the next unrelated tool's `PostToolUse`
  pairs with the stale id.
  - **Decided at green:** option (a), see "Decisions taken at green" below.

## Decisions taken at green

The developer was AFK during `/green`, so each call below was taken here and is recorded for review.

1. **Stale pending call: option (a).** `agent_activity_request` returns `None` for a `PreToolUse` of
   `AskUserQuestion`. Nothing is pushed as pending and no `running` row is written. Pinned by
   `a_pre_tool_use_of_ask_user_question_is_not_reported_as_a_tool_call` (`tddy-tools`).
   - The `PostToolUse` of `AskUserQuestion` is still reported, as it always was. It appends a
     `completed` row under a fresh id, because nothing is pending to pair with. **Open for the human:**
     whether that lone completed row should go too.
2. **The question is recorded by `ReportSessionStatus`, validated before any write.** A malformed
   `AskUserQuestion` (input that is not JSON, or names no question) is refused as `InvalidArgument`
   before the activity status, the pending file or the flag is written.
3. **`.session.yaml` `pending_elicitation` is now written for claude-cli.** It is true while a question
   is recorded or kept, and false on a clear. `set_pending_elicitation` writes only when the flag
   changes. It is new in `tddy-changeset` and is not in the contract's list.
4. **The kept follow-up returns an empty `request_id`.** The `Notification` that follows a prompt
   records nothing new. The hook ignores the response in any case.
5. **Keep rule, with no message.** A `WaitingForInput` with no tool keeps a pending structured question
   even when its message is empty. It fills the question's text from the message only when that text is
   empty. A `WaitingForInput` with no tool and no pending structured question keeps the `#start-parity`
   6/9 behaviour: a message is recorded, and no message clears.
6. **Once per request, in the Telegram subscriber.** `TelegramDaemonHooks` records `question:<request_id>`
   in its per-session last-delivered memory. The key is namespaced so it cannot collide with a status
   text. Pinned by `a_permission_alert_names_the_tool_and_its_input_summary_once_per_request`.
7. **Wording.** A permission reads `Run <tool>: <summary>`. An empty summary reads `Run <tool>`, with no
   trailing colon. Drawer text is `🔔 Session {label} wants to run {tool}: {summary}`, or
   `🔔 Session {label} asks: {first question}`.
8. **`summary()`.** Newlines become spaces and the result is trimmed. `Bash` names its `command`;
   `Edit`, `Write` and `Read` name their `file_path`. Any other tool, or a missing field, gives the input
   as compact JSON. Input that is not JSON is used as the raw text.
9. **Alert layout, as pinned.** The "(choose several)" hint is appended to a multi-select question. The
   header line is omitted when the header is empty. Only `&`, `<` and `>` are escaped. The kernel's
   `escape_html` also escapes `"`, which the pinned layout does not allow, so `question_alert` has a
   local escaper.
10. **Delivery.** The alert is sent loud (`silent: false`) with `send_html_message`, to the tracking chats
    or else the broadcast list, as the status alert is.
11. **Public surface added beyond the contract's list.** `tddy_changeset::session_activity::
    {ASK_USER_QUESTION_TOOL, hook_event_asks_a_question}` and `tddy_changeset::session_metadata::
    set_pending_elicitation`, all re-exported from `tddy-changeset`. `tddy-tools` and
    `tddy-session-activity` need them.
12. **`streams.rs`.** The `..Default::default()` is dropped. All twelve fields are set, `question`
    explicitly.

**Open for the human:**

- **The question alert is one message.** A long question set can exceed Telegram's 4,096-byte limit.
  The Bot API would refuse it, and the bus would log the failure. It is not split, because the pinned
  `QuestionAlert::html()` contract is one message. Splitting with `AgentNotificationTelegram`'s
  splitter is the follow-up.
- The `PostToolUse` completed row for `AskUserQuestion` (decision 1).

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-elicit-content-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (reviewed by the orchestrator, developer AFK, 2026-10-26)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (2026-10-28, see "Decisions taken at green")
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
