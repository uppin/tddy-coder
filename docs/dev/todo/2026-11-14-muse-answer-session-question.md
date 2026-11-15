# 2026-11-14 — `AnswerSessionQuestion` cannot answer a Muse Code question

**Category:** Future enhancement
**Source:** project-home orchestrator driving `muse-cli` session `01a1d89b-0e66-7880-bed2-a6bec7ba7238`
on 2026-11-14

For Claude Code, an orchestrator's answer travels back through the blocking
`PreToolUse[AskUserQuestion]` hook: it runs `tddy-tools session-hook --await-answer` and prints the
decision Claude reads (`packages/tddy-tools/src/session_hook.rs`,
`packages/tddy-tools/src/session_hook/await_answer.rs`). A Muse session's hooks are report-only —
its `PreToolUse` never fires for `request_user_input` (probe 2026-11-15, recorded in
`docs/dev/1-WIP/2026-11-15-muse-hooks-wired.md`) — so there is no pending question with a
`request_id` and nothing to answer. Today the only way is `ClaimSessionTerminal` +
`SendSessionKeys` on Muse's selection UI, which worked on 2026-11-14.

## Done (2026-11-15)

`AnswerSessionQuestion` on a session whose agent declares no `ask_user_question` is refused
naming the agent's product and pointing at `ClaimSessionTerminal` + `SendSessionKeys`
(`packages/tddy-daemon-rpc/src/exec_tool/project_home/answer.rs`,
changeset `docs/dev/1-WIP/2026-11-15-muse-hooks-wired.md`).

## What remains (Later, one of)

- MSP: `muse serve` (stdio, or a `--listen` socket) is Muse's session protocol. Its events include
  `UserInputRequested` and `ApprovalRequested`; `muse schema` exports the wire schema. A full
  protocol integration, comparable to ACP.
- The daemon types the answer: map each answer to its option index in the selection UI and send
  keys. Agent-specific and fragile across Muse releases.
- A hook answer — disproven on Muse 1.4.12 (`PreToolUse` never fires for `request_user_input`,
  so no hook can carry the answers back); re-probe only if Muse's hook behavior changes.

**Why deferred:** the PTY path already works; the refusal above says so.
