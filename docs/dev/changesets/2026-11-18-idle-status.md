# 2026-11-18 — An idle session waits for input, and a message to it is delivered

**Type:** Fix · `#coder-start` 6/10, PR [#946](https://github.com/uppin/tddy-coder/pull/946)
**Packages:** `tddy-session-activity`, `tddy-daemon-rpc`, `tddy-changeset` (doc only)

A session idle at its prompt after a turn was reported with a pending question of `kind: permission`
and an empty `request_id`: Claude's `idle_prompt` `Notification` maps to `WaitingForInput` with no
tool, and `record_pending_question` wrote the message as an id-less record. `SendSessionMessage`
then refused the session (`waiting on permission ""`), `AnswerSessionPermission` had no id to
answer, and an idle wake never fired.

- `tddy-session-activity` (`service/hook_reports.rs`): a `WaitingForInput` with no tool and no
  structured question still pending writes **no** record — the session is `WaitingForInput` and the
  plain `ActivityStatus` notification is still published. A pending question is only ever written
  with a `request_id`.
- `tddy-daemon-rpc` (`exec_tool/project_home/turn.rs`): `refuse_while_waiting_on_an_answer` refuses
  only a record with a `request_id`; an id-less record left on disk from before the fix has nothing to
  answer and does not refuse (the next `PostToolUse`/`UserPromptSubmit`/`Stop` clears it anyway).
- `tddy-changeset`: `PendingQuestion::request_id` documented as never empty by design.
- Idle wakes needed no change: `watch_wake.rs` already treats only a present record as not idle.

Structured questions (`PermissionRequest`, `AskUserQuestion`) and the `Notification` that follows
them are unchanged. No proto change.

**Pinned by** `an_idle_prompt_leaves_the_session_waiting_for_input_with_nothing_to_answer`
(`pending_question_capture_acceptance`), `a_message_to_a_session_waiting_for_input_is_delivered`
(`project_home_turn_control_acceptance`), `a_session_waiting_for_input_with_nothing_to_answer_wakes_as_idle`
(`project_home_watch_wake_acceptance`).

**Backlog:** narrows *An idle session is reported as waiting on a permission with an empty request id*
(`2026-11-18-idle-session-reported-as-waiting-on-a-permission`) to its two unreproduced terminal and
activity-read observations; the permission-wait part is closed here.
