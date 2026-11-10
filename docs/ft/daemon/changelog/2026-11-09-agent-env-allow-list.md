# 2026-11-09 — Agents start from an allow-listed environment

An agent process now starts from an allow-listed environment instead of the daemon's own.

- The daemon passes an agent its base variables (`PATH`, `HOME`, `USER`, `LANG`, `SSH_AUTH_SOCK`, proxy and `XDG_*` variables, and the like), the variables its spec names in `pass_env`, and what the launch sets. Nothing else reaches it — no `CLAUDE_CODE_*` marker, CI token or other credential the daemon was started with.
- The built-in specs name their provider keys: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `CURSOR_API_KEY`.
- An operator adds a variable with `pass_env` in `tools/<agent>.yaml`, or replaces the base set with `tools/agent-env/base.yaml`. A `TDDY_*` entry in either is refused when the file loads.
- Applies to headless turns, ACP agents, model listing, host PTY and tmux sessions, split agents and agents the supervisor spawns as another user. A user's own terminal still inherits the daemon environment.
