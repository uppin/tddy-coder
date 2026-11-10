# 2026-11-07 — One coding-agent registry per daemon, and a broken tool file stops the start

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- **A broken `tools/<agent>.yaml` stops the daemon.** The registry (built-in specs plus
  `<tddyhome>/tools`) is loaded once at start; a file that does not parse fails the start with an
  error naming it, whatever is configured — Telegram no longer quietly offers no agents.
- **One registry, everywhere.** The session host, Telegram, the reserved assistant ids and the model
  catalog read the same instance; nothing turns a missing registry into an empty one.
- **Model overrides show up.** An operator's `models:` override in `tools/<agent>.yaml` is the list
  `ListAgentModels` returns (`tddy-tools list-models --tools-dir`).
- **The PTY sandbox offers only agents it can run.** `tddy-sandbox-app --agent` no longer lists
  `claude-acp` / `codex-acp`; choosing one is refused, naming the agents it can run.

See [Session agent roster](../session-agent-roster.md) and
[Telegram session control](../telegram-session-control.md).
