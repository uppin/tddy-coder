# 2026-11-03 — A tmux-run agent's environment travels on tmux's argv, provider token included

**Category:** Security / spawn path
**Source:** `#tmux-agent 1/3`, PR #814, `/validate-changes` 2026-11-03

- `tddy_cli_sessions::tmux::start_agent_tmux_session` sets the agent's environment on the pane with
  `tmux new-session -e K=V …` (`agent_new_session_args`). That environment is the session's whole
  `env_extra`: the notify/toolcall variables **and the provider route's `ANTHROPIC_AUTH_TOKEN`**
  (`claude_cli_spawn.rs`, `provider_route.env`).
- An argv is readable by every local user (`ps`, `/proc/<pid>/cmdline`) for as long as the
  `tmux new-session` client runs. It is short-lived (the client exits once the detached session
  exists), but the token is a credential, and the PTY path never exposes it: there the environment is
  the child's process environment.
- `-e` was the developer's decision ("Env via `new-session -e`", changeset Decisions) because a pane
  inherits the tmux *server's* environment, not the client's; the argv exposure was not weighed there.
- Every tmux CLI channel is argv (`set-environment` included), so the fix is a file: write the
  environment as a `0600` file owned by the session's OS user, and start the pane as
  `sh -c '. "$ENVFILE"; rm -f "$ENVFILE"; exec "$@"' sh <claude argv>`, or an equivalent tddy helper
  that reads and unlinks it. Needs the developer's choice of channel and where the file lives
  (session directory vs worktree), and it applies to `#tmux-agent 3/3`'s harnesses too.

**Recommendation:** follow-up PR on top of the stack (or folded into `#tmux-agent 3/3`, which owns the
other harnesses' tmux env). Not fixed in #814: it reverses a recorded developer decision.
