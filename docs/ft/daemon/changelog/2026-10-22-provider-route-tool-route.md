# 2026-10-22 — Tool sessions on a provider account

A tool session whose agent is Claude (`claude`, `claude-acp`) or Codex (`codex`, `codex-acp`) can
start and resume on a provider account. Its route reaches `tddy-coder` in an owner-only file, never on
the command line; Codex runs the account as a Responses provider. Cursor, the stub and specialized
agent definitions are refused. See
[provider accounts § Tool sessions](../provider-accounts.md#tool-sessions).
