# 2026-11-21 — A claude-cli session's hooks report as that session after a resume

Each claude-cli session has a hooks file of its own (`.tddy/hooks/<session_id>/claude-settings.json`
in its worktree), written on every start and resume and loaded with `--settings`. Sessions sharing a
worktree, as the project-home session and the project's orchestrators do, report as themselves.
A resume after a daemon restart reports over the running daemon's socket, so tool calls and `/clear`
are recorded and the next resume opens the latest conversation. The daemon's old entries in a
worktree's `.claude/settings.local.json` are removed on launch; Claude Code's own settings there are
kept. See [claude-cli-session.md](../claude-cli-session.md).
