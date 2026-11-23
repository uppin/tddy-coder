# 2026-11-23 — `run_in_tmux` on a Coder/SSH target runs the agent in tmux on the target

- A target session started with `run_in_tmux` runs its agent in `tmux new-session -A -s tddy-<session>` on the target, so it survives a dropped connection; the session lists inactive and Resume re-attaches to it (or resumes the agent in tmux when it has ended).
- A target without `tmux` on its login `PATH` refuses the start or resume `FAILED_PRECONDITION` naming tmux — no fallback.
- No pid file for a tmux run: the tmux session name is the identity; the session pointer's owner is the pane's pid.
- Stop detaches; Delete kills the tmux session on the target.

Feature: [terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals)
