# 2026-11-21 — Muse `turn_input: terminal_paste` is declared, not observed

**Category:** Verification gap
**Source:** `#orchestra` 7/9 (PR #985), wrap
**Consent:** the developer chose to defer this on 2026-11-22 at the `#orchestra` stack's TODO gate.

`cursor.yaml` and `muse.yaml` now declare `turn_input: { kind: terminal_paste, submit_key: Enter }`,
so `SendSessionMessage` pastes into their TUI and presses Enter, as it does for Claude. The tests
(`packages/tddy-coding-agent-specs/tests/spec_turn_input_delivery_acceptance.rs`) prove the
declaration loads; nothing in the codebase or its notes records either TUI being driven this way.
Claude's paste delivery was checked live (`#orchestrators` 4/11). Cursor's was checked live on
2026-11-24 (PR #1028, Cursor Agent 2026.11.22, on a local pty, local tmux and `coder ssh` → tmux on
`remote-dev`): with the submit key written after the paste's echo, a single-line paste, a
multi-line paste (submitted once) and a paste while a turn runs (queued as a follow-up) all submit.
Muse's has not been checked.

## What to verify, per agent, against a live session

- **Cursor** — one item left: a paste into an approval dialog goes through unrefused (Cursor
  reports no `WaitingForInput`, `docs/ft/daemon/cursor-cli-session.md`). The 2026-11-24 runs used
  `Run Everything`, which shows no approval dialog, so this was not exercised.
- **Muse** — against Muse's TUI: a bracketed paste of a multi-line message, then `Enter`,
  submits once, not once per line; a paste while a turn runs queues instead of being dropped; a
  paste into an approval dialog. Muse's hooks are report-only and its
  `request_user_input` fires no `PreToolUse` (`2026-11-14-muse-answer-session-question.md`), so a
  paste while Muse shows its selection UI would land in that UI, unrefused.

## What would close it

A live run per agent with the outcome recorded here; if `Enter` is not the submit key, or a
multi-line paste submits early, change that spec's `submit_key` (or drop its `turn_input`) and say
why in the spec comment.
