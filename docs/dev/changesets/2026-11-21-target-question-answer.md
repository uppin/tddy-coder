# 2026-11-21 — A target agent's question is answered by its orchestrator

**Type:** Feature · **Stack:** `#target-parity` 6/7 (PR #974) · **Packages:** `tddy-agent-backend`,
`tddy-agent-launch`, `tddy-session-activity`, `tddy-tools` ·
Feature: [claude-cli-session.md § On a target session](../../ft/daemon/claude-cli-session.md)

## Problem

After `#target-parity` 1/7 a target answered permission prompts through `.tddy/answers/<nonce>`, but
the `PreToolUse` hook matched to `AskUserQuestion` was skipped there, so a remote agent's question
appeared only in its terminal and an orchestrator could not answer it.

## What changed

- **`tddy-agent-backend` `claude_hooks.rs`**: on a target every waiting hook (permission and question)
  gets `remote_answering_hook_command`; the `continue` that skipped the question entry is gone.
- **`tddy-agent-launch` `remote_hook_answer.rs`**: `HeldHook::question_input` (from
  `HookLine::question_input`) selects the decision kind; a question is answered with
  `question_decision_json` over the call's `tool_input`, a permission as before.
  `remote_hook_follower.rs` grew by one line (516 → 517).
- **`tddy-session-activity` `hook_decision.rs`**: `question_decision_json` and
  `encode_question_decision` moved here from `tddy-tools` (`session_hook/question_decision.rs` is
  deleted; `await_answer.rs` imports them); their tests moved to
  `tests/question_decision_unit.rs`.

## Decisions & Trade-offs

- **Move, not duplicate**: only daemon-side crates reach a target's shell, and `tddy-tools` already
  depends on `tddy-session-activity`, so the encoding has one home.
- **A question line with no `tool_input`** holds as a question with `null` input: the encoding then
  yields no decision, an empty answer file, and Claude's own dialog — never a permission decision.

## Tests

`remote_hooks_acceptance`: `a_question_of_an_agent_on_a_target_is_held_for_its_orchestrators_answer`,
`an_orchestrators_answer_to_a_target_agents_question_is_printed_by_its_hook`,
`a_question_nobody_answers_leaves_the_target_agent_to_ask_in_its_dialog`.
`remote_hooks_render_acceptance` expects `PreToolUse` among a target's waiting events.
Units: `a_target_waits_for_a_question_answer_beside_its_hook_log` (claude_hooks),
`a_question_wait_nobody_answered_hands_nothing_to_claude` (hook_decision), and the moved
`question_decision_unit` suite.
