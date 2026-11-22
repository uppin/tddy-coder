# 2026-11-21 — Cursor and Muse `turn_input: terminal_paste` is declared, not observed

**Category:** Verification gap
**Source:** `#orchestra` 7/9 (PR #985), wrap

`cursor.yaml` and `muse.yaml` now declare `turn_input: { kind: terminal_paste, submit_key: Enter }`,
so `SendSessionMessage` pastes into their TUI and presses Enter, as it does for Claude. The tests
(`packages/tddy-coding-agent-specs/tests/spec_turn_input_delivery_acceptance.rs`) prove the
declaration loads; nothing in the codebase or its notes records either TUI being driven this way.
Claude's paste delivery was checked live (`#orchestrators` 4/11); these two were not.

## What to verify, per agent, against a live session

- **Cursor** (ink TUI, `→ Add a follow-up` prompt) — the list in
  [`2026-11-16-cursor-hooks-udoo.md`](2026-11-16-cursor-hooks-udoo.md) § Side finding:
  - a bracketed paste of a multi-line message, then `Enter`, submits once, not once per line;
  - a paste while a turn is still `Composing` queues as a follow-up instead of being dropped;
  - a paste into an approval dialog goes through unrefused (Cursor reports no `WaitingForInput`,
    `docs/ft/daemon/cursor-cli-session.md`) — acceptable under `Run Everything`, otherwise it needs
    a guard.
- **Muse** — the same three, against Muse's TUI. Muse's hooks are report-only and its
  `request_user_input` fires no `PreToolUse` (`2026-11-14-muse-answer-session-question.md`), so a
  paste while Muse shows its selection UI would land in that UI, unrefused.

## What would close it

A live run per agent with the outcome recorded here; if `Enter` is not the submit key, or a
multi-line paste submits early, change that spec's `submit_key` (or drop its `turn_input`) and say
why in the spec comment.
