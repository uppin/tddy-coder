# 2026-10-24 — Headless coding agents as subagents

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)).

- **Claude Code (`claude -p`), Cursor (`cursor agent -p`), Codex (`codex exec`) and Warp (`oz`)
  harness profiles can be attached as coding subagents**, driven headless one CLI run per turn, with
  the same tools, outcome and activity bubble as an ACP agent.
- **Every coding-agent row says its transport and its permission policy**, in the pickers too; an
  agent offered both ways is listed once per transport.
- **A headless Claude's permission prompt yields to the main agent** like an ACP one; Cursor, Codex
  and Warp run under the fixed policy their row names.
- Headless agents open `clone`-placed, as the daemon's own user, in unmanaged sessions.

Feature doc: [Session agent roster § Coding agents](../../daemon/session-agent-roster.md#coding-agents).
