# 2026-11-05 — The generic CLI launch serves what Claude Code and Cursor need

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)).

- **An agent with Claude Code's or Cursor's needs is a YAML file.** A copy of `claude.yaml` or
  `cursor.yaml` dropped into `<tddyhome>/tools/` under another id starts through the generic CLI
  launch with the argv, files and environment the built-in `claude-cli` / `cursor-cli` launches
  produce: in tmux, as a project's main, on an ssh checkout and in a jail.
- **A managed codebase follows the `managed_workflow` capability**, declared by `claude.yaml` and
  `cursor.yaml`. A managed start of an agent without it is refused, naming the agent.
- **`coding_agent.conversation`** declares how a session's conversation is identified: the daemon's
  session id (Claude Code), an id a launch adapter mints before the first start (Cursor's
  `create-chat`), the worktree (OpenCode, Muse) or a resume token (Warp).
- **No hang on a start**: Cursor's chat mint is bounded by 30 s and an ssh slot-file write by 60 s;
  either refuses the start `DEADLINE_EXCEEDED` naming the agent or the ssh host.
- **The agent process sees no `TDDY_*` variable it was not given**, including in a tmux pane, which
  inherits the tmux server's environment.

Known gaps, owned by later `#agent-tools` nodes: a managed codebase in a jail is refused on the
generic launch (n17 carries it there before deleting the bespoke jailed claude start), and the
built-in `claude` and `cursor` ids still take their bespoke launches until n17.
