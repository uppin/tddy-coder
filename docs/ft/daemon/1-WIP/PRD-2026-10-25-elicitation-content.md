# What a claude-cli session is asking - PRD

**Date**: 2026-10-25 (replanned 2026-10-26)
**PRD Type**: Enhancement
**Stack**: `#home-inbox 3/8` (branch `feature/home-inbox/elicit-content`)

## Affected Features

- **Primary**: [Claude Code CLI session](../claude-cli-session.md) - two more hooks report what a permission prompt or a question asks
- **Related**: [Project concept § Project-home tools](../project-concept.md) - `ReadSessionActivity`'s pending question (`#start-parity 6/9`) carries the tool, its input and every option
- **Related**: [Session notifications](../session-notifications.md) - an attention notification can carry the pending question
- **Related**: [Telegram session notifications](../telegram-notifications.md) - the alert names the tool and the question, not just "needs your input"

## Summary

When a claude-cli session waits for a permission or asks a question, the daemon learns **what** it
is asking. It records that in the session's **pending question**, the record `#start-parity 6/9`
introduced, under a stable `request_id`:

- **permission:** the tool name and its input;
- **question:** each question's text, its options, and whether several can be chosen.

The session's `pending_elicitation` becomes true. The notification carries the question, and
Telegram's alert spells it out. An orchestrator reading `ReadSessionActivity` sees the same record.
Answering is `#home-inbox 6/8` and `8/8`. This node only reports.

## Background

`#start-parity 6/9` records a session's pending question. For a claude-cli session, all it has is
the `Notification` hook's `message`, such as "Claude needs your permission to use Bash". The tool's
input, and an `AskUserQuestion`'s questions and options, never reach the daemon. No
`PermissionRequest` or `PreToolUse` hook is installed. Nothing writes `pending_elicitation` for a
claude-cli session.

## Proposed Changes

### What's Changing

- **Two new hooks** are installed in a claude-cli worktree:
  - `PermissionRequest`;
  - `PreToolUse` with the matcher `AskUserQuestion`.

  Both report `WaitingForInput` with the tool name and its input, and **return immediately, with no
  decision**. Claude behaves exactly as it does today.
- **The pending question gains structure.** It keeps `kind`, `header`, `question`, `options` and
  `allow_other` and adds:
  - `request_id`;
  - `tool_name`;
  - `tool_input_json`;
  - `questions: [{question, header, options: [{label, description}], multi_select}]`;
  - `at`.
- **Keeping.** The `Notification` hook that follows the same prompt keeps the structured record and
  doesn't replace it with its message.
- **Clearing.** The session's next `PostToolUse`, `UserPromptSubmit`, `Stop` or `SessionEnd` clears
  the record and resets `pending_elicitation`. That is what `#start-parity 6/9` already does for any
  non-waiting status.
- **Telegram alert.**
  - A permission reads "*Session X wants to run* `Bash`: `<command summary>`".
  - A question gives the question text and lists its options.
  - Each alert is sent once per `request_id`.

### What's Staying the Same

- Claude's own terminal prompt remains the only place a person answers in this node.
- `ReadSessionActivity`'s shape and paging. It shows more fields because the record has more.
- The existing `WaitingForInput` status and its alert. The question alert replaces it for the same
  prompt, so the operator isn't told twice.

## Impact Analysis

- **Technical.**
  - The hook set grows from 6 to 8 events.
  - `ReportSessionStatus` carries the tool and its input.
  - The pending-question record gains optional fields.
  - One notification builder.
- **User.** "Session X wants to run `cargo publish`" instead of "needs your input".

## Acceptance Criteria

- [ ] A `PermissionRequest` payload records the tool name and input as the pending question, sets
  `pending_elicitation`, and publishes it once.
- [ ] An `AskUserQuestion` `PreToolUse` payload records every question and option.
- [ ] The `Notification` hook that follows keeps the structured record.
- [ ] The next `PostToolUse`, `UserPromptSubmit`, `Stop` or `SessionEnd` clears it and resets
  `pending_elicitation`.
- [ ] `ReadSessionActivity` shows the tool and its input for a claude-cli permission.
- [ ] Both hooks return within the hook budget with empty stdout, so Claude's own prompt still
  appears.
- [ ] Telegram's alert names the tool and its input summary, or lists the question and options, and
  is sent once per request. No second "needs your input" alert follows for the same prompt.
- [ ] A wrong hook token is refused and records nothing.

## Out of scope

- Answering (`#home-inbox 6/8`, `8/8`).
- Sandboxed and cursor-cli sessions.
- `tool` (presenter) sessions. `#start-parity 6/9` already records their questions.

## Changes from the 2026-10-25 plan

Replan of 2026-10-26. `#start-parity 6/9` landed a pending-question record first, so this node
extends it. It no longer creates its own elicitation record, `elicitations.json` or
`ReportElicitation` RPC. Ids are `request_id`.
