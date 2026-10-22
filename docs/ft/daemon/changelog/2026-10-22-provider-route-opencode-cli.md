# 2026-10-22 — OpenCode terminal sessions

A third CLI session type, **OpenCode** (`opencode-cli`): OpenCode's TUI in a PTY in a daemon-cut
worktree, resumable with `opencode --continue`, its models listed from `opencode models`, its binary
configurable under `cli_harnesses.opencode-cli.binary_path`. On a provider account that serves OpenAI
Responses or Chat Completions it runs on a generated `opencode.json` naming that account, with the key
only in the child's environment. Sandboxed and managed-codebase starts are refused. See
[OpenCode CLI session](../opencode-cli-session.md).
