# 2026-11-04 — One coding-agent registry; agent gates are spec capabilities

`#agent-tools` 12/19 ([#801](https://github.com/uppin/tddy-coder/pull/801)).

- **Breaking: daemon.yaml `claude_cli:`, `cursor_cli:` and `cli_harnesses:` are refused at start-up.**
  The error names the tool file that replaces each (`<tddyhome>/tools/<id>.yaml`) and where each field
  went (`binary_path` → `binary.command`, `claude_home_dir` → `home.key`, `fullscreen` → `run.env`,
  `answer_wait_secs` → `coding_agent.answer_wait_secs`).
- **A new agent is a YAML file.** An agent dropped into `<tddyhome>/tools/` that names existing
  adapters appears in the host's agent list and starts as an `<id>-cli` session, on the host or in a
  jail, offering the capabilities it declares.
- **Hook activity, question answers, operator notifications, split placement, the required model and
  worktree removal on delete follow the agent's declared `capabilities`**, not its session type's
  name: removing `hooks` from a `claude.yaml` override turns off hook-based activity for Claude.
- **The agent catalog reports each agent's kind, harness and route support**, and the create-session
  pane asks for provider routes only for an agent that can be routed.
- **`tddy-coder`'s backend selection lists the registry**: every agent with a headless or ACP mode,
  then `stub`.

Known gaps, owned by later `#agent-tools` nodes: provider routes are still chosen per built-in
harness, so a dropped-in agent cannot be routed (`route-support`); Claude and Cursor keep their own
launch paths (`cli-spec-features`, `cli-dispatch-collapse`); `tddy-coder --agent` builds only the agents
with a backend adapter (`spec-backend`); a Telegram-initiated delete leaves an agent session's worktree
on disk (`agent-config-removal`).

See [Session agent roster](../session-agent-roster.md) and [Harness profiles](../harness-profiles.md).
