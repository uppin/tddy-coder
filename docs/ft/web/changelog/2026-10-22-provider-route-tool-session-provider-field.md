# 2026-10-22 — The Provider field for tool sessions

A tool session's Create-session form shows the **Provider** field for the agent it picked: a `claude`,
`claude-acp`, `codex` or `codex-acp` session can run on a connected provider account, and `cursor`
shows why it cannot. Changing the agent resets the choice. See
[tool-session model selection § Provider](../tool-session-model-selection.md#provider).
