# 2026-11-10 — Agent usage declared in YAML; Claude numbers corrected

`#usage-binding` 1/5 ([#890](https://github.com/uppin/tddy-coder/pull/890)).

- **A coding agent spec declares its usage source** under `coding_agent.usage`; one agent-agnostic
  engine reads it. Claude and Claude-ACP are the first bindings.
- **Claude usage is correct**: a message written as several transcript lines is counted once, and
  cache-read and cache-write tokens are reported per conversation and in the totals.
- **Subagents of subagents** are accounted, each under its parent.
- **Breaking:** a `<tddyhome>/tools/claude.yaml` override still carrying `transcript_usage:` is
  refused at load; replace it with a `usage:` section.

See [Session token accounting](../session-token-accounting.md).
