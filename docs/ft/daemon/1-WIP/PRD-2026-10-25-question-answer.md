# Project-home agent answers a session's question - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 8/8` (branch `feature/home-inbox/question-answer`)

## Affected Features

- **Primary**: [Claude Code CLI session](../claude-cli-session.md) - an orchestrated session's `AskUserQuestion` waits, for a bounded time, for its orchestrator's answers
- **Related**: [Project concept § Project-home tools](../project-concept.md) - `AnswerSessionQuestion`

## Summary

When an orchestrated child asks a question with Claude's `AskUserQuestion` tool, the home agent
answers it:

```
AnswerSessionQuestion(session_id, request_id,
                      answers: [{ question_index, selected: ["<label>", …], other? }])
```

The child continues with those answers. As with permissions (`#home-inbox 6/8`), the wait is bounded.
With no answer in time, the child shows its own question dialog.

## Background

Claude's hooks can't fill in a question dialog directly. A `PreToolUse` hook on `AskUserQuestion`
can do one of two things:

- **(a)** return `updatedInput` with the answers, on Claude builds that accept answers in the tool
  input;
- **(b)** deny the tool with a reason that states the answers, which the model reads as the user's
  reply.

This node picks the mechanism after verifying it against the installed Claude Code in pass 2, and
records the choice. (b) works on every build but leaves a "denied" entry in the transcript.

## Proposed Changes

### What's Changing

- **The waiting hook.** In an orchestrated child, the `PreToolUse` hook for `AskUserQuestion` waits
  for the home agent's answers in the same bounded way as permissions. On an answer it prints the
  chosen decision JSON.
- **Validating answers.** `AnswerSessionQuestion` (project-home tool) accepts `answers` for a question, under the same targeting rules as `AnswerSessionPermission`.
  It refuses an answer that names a question index the pending question doesn't have, or selects an
  option label it doesn't offer (unless `other` text is given). It also refuses several selections
  on a single-choice question.
- **Recording.** The answers are recorded on the pending question and in both sessions' logs.

### What's Staying the Same

- Questions with no orchestrator, or no answer in time, behave exactly as today.
- Permissions (`#home-inbox 6/8`).

## Impact Analysis

- **Technical.** Reuses `6/8`'s pending-answer registry and long poll. Adds answer validation and
  the decision encoding for questions.
- **User.** A child's clarifying question is answered by its orchestrator without a person.
- **Risk.** The decision format depends on the Claude Code version. The node's tests pin the format
  it emits, and its changeset records the build it was verified on.

## Acceptance Criteria

- [ ] An orchestrated child's `AskUserQuestion` hook, answered by its home agent, prints the decision
  that hands Claude exactly those answers.
- [ ] An answer naming an unknown question index or option label, or making several selections on a
  single-choice question, is refused and the hook keeps waiting.
- [ ] With no answer before the deadline, the hook prints nothing.
- [ ] `AnswerSessionQuestion` on a permission request is refused, naming `AnswerSessionPermission`.

## Out of scope

- Free-text-only prompts that don't come from `AskUserQuestion`.
- Answering from Telegram or the web.

## Changes from the 2026-10-25 plan

Replan of 2026-10-26: `master` moved, and this stack now sits on `#start-parity 9/9` (#758).

- `AnswerSessionQuestion` takes an optional `daemon_instance_id`, which forwards the call to the
  child's host. As in `6/8`, the hook waits only while the orchestrator runs on that same host.
- Questions are `#start-parity 6/9`'s pending-question records, as extended by `3/8`. They are
  addressed by `request_id`. A `tool` session's clarification, which has no `request_id`, is refused.
