# 2026-07-12 — Specialized-agent warm-up gate

**Type:** Feature

`start_sandboxed_claude_cli_session` and `start_sandboxed_cursor_cli_session` call `tddy_discovery::warmup::warm_up_agents` right after resolving the specialized-agent defs and before spawning the jail; a cold/unreachable agent endpoint fails the start with `FAILED_PRECONDITION` (naming the agent/endpoint/model) instead of stalling the main agent's first `subagent_prompt` — no fallback. `502`/`5xx`/`429`/connection-errors retry to a 120s budget, `404` fails fast; resume reuses the start path so it is gated too. Architecture [connection-service.md § Sandboxed Claude Code CLI sessions](../connection-service.md#sandboxed-claude-code-cli-sessions). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#296](https://github.com/uppin/tddy-coder/pull/296). (tddy-daemon, tddy-discovery, tddy-sandbox-app)
