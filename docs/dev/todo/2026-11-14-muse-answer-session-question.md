# 2026-11-14 — `AnswerSessionQuestion` cannot answer a Muse Code question

**Category:** Future enhancement
**Source:** project-home orchestrator driving `muse-cli` session `01a1d89b-0e66-7880-bed2-a6bec7ba7238`
on 2026-11-14

For Claude Code, an orchestrator's answer travels back through the blocking
`PreToolUse[AskUserQuestion]` hook: it runs `tddy-tools session-hook --await-answer` and prints the
decision Claude reads (`packages/tddy-tools/src/session_hook.rs:240-258`,
`packages/tddy-tools/src/session_hook/await_answer.rs`). A Muse session has no hooks and so no
pending question with a `request_id`
([2026-11-14-muse-hooks-not-wired.md](2026-11-14-muse-hooks-not-wired.md)), so there is nothing to
answer. Today the only way is `ClaimSessionTerminal` + `SendSessionKeys` on Muse's selection UI,
which worked on 2026-11-14.

## What remains

- **Now:** `AnswerSessionQuestion` on a session whose agent cannot take a structured answer should
  say so (and point at the terminal tools) rather than leave the caller guessing.
- **Later, one of:**
  - A hook answer: a blocking `PreToolUse[request_user_input]` returning the answers. Unproven. Muse
    knows `updatedInput`, but its strings show it limited by a "correction budget", and whether
    `PreToolUse` fires for `request_user_input` at all is the probe in the hooks item.
  - MSP: `muse serve` (stdio, or a `--listen` socket) is Muse's session protocol. Its events include
    `UserInputRequested` and `ApprovalRequested`; `muse schema` exports the wire schema. A full
    protocol integration, comparable to ACP.
  - The daemon types the answer: map each answer to its option index in the selection UI and send
    keys. Agent-specific and fragile across Muse releases.

**Why deferred:** depends on the hooks probe; the PTY path already works.
