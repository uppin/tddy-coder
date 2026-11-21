# 2026-11-21 — A claude-cli session's hooks are a file of its own, rendered on every launch

**Type:** Fix
**Packages:** `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-split`

`/clear` in the project-home session was never recorded, and the next resume reopened the
conversation from before the clear (reported 2026-11-21, session `01a16dc6-…`). The rotation was
sound. No hook of the session reached the daemon:

- Claude's hooks were one worktree-wide `.claude/settings.local.json` naming a session id, its token
  and the daemon's agent-tool socket.
- A resume re-rendered only argv-delivered hooks.
- An orchestrator started in the same project-home worktree had taken the file over, and the socket
  it named (`tddy-agent-tools-<daemon pid>-…`) had died with a daemon restart.
- `tddy-tools session-hook` is fail-quiet.

A daemon restart alone broke hooks for any resumed claude-cli session.

**What changed:**

- Each claude-cli session's hooks are `<worktree>/.tddy/hooks/<session_id>/claude-settings.json`,
  loaded with `--settings`, and the route's `env` pin lives there too.
- `resumed_hooks` renders every adapter's hooks on every resume.
- `session_hooks_worktree` git-excludes `/.tddy/hooks/` and releases the daemon's entries from the
  legacy `settings.local.json`, locally or through a target's shell.
- Split sessions keep their file in their private context dir.

Reproduced by `a_resume_after_a_daemon_restart_rewires_the_hooks_to_the_live_agent_tool_socket` and
`a_session_started_in_the_same_worktree_leaves_a_resumed_sessions_hooks_its_own`. Deferred:
`docs/dev/todo/2026-11-21-tmux-agent-keeps-hooks-of-an-exited-daemon.md`.

Docs: [claude-cli-session.md](../../ft/daemon/claude-cli-session.md) § Session activity status via
per-session hooks.
