# 2026-10-24 — ACP backends honour the model, cancel, and end their agent

`#coagents` 1/8 ([#720](https://github.com/uppin/tddy-coder/pull/720)).

- **The chosen model takes effect** on `claude-acp`, `codex-acp` and `opencode` (off a provider
  route). A model the agent does not offer fails the run naming the ones it does.
- **Cancel stops the agent's turn**, which ends as cancelled.
- **Dropping a backend ends its agent process.**
- **`claude-acp` resumes by session id** across a backend restart, like the other two.
- Permission requests are still allowed once; Codex's `codex login` retry is unchanged.

Feature docs: [OpenCode ACP backend § Behavior](../opencode-acp-backend.md#behavior),
[Codex ACP backend](../codex-acp-backend.md).
