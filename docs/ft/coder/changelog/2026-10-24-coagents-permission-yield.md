# 2026-10-24 — A coding subagent's permission request is the main agent's to answer

`#coagents` 3/8 ([#722](https://github.com/uppin/tddy-coder/pull/722)).

- **A coding subagent's permission request yields the turn** with `stopReason: "permission_requested"`
  and the request's tool call and options; the subagent waits, paused.
- **`subagent_resume { permission }` answers it** with an option or as cancelled, and returns the rest
  of the same turn.
- **One question at a time**: a prompt, or a resume that does not answer the pending request, is
  refused naming it.
- **Cancelling while one is pending** answers it as cancelled and ends the turn `cancelled`.
- Works the same when another daemon serves the conversation. The tool kind reads `other` for now.

Feature doc: [Managed-codebase subagents](../managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer).
