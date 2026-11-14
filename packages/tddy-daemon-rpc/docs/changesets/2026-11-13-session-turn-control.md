# 2026-11-13 — Orchestrators drive a session's turns

**Type:** Feature

`#orchestrators 4/11` (PR [#881](https://github.com/uppin/tddy-coder/pull/881)). Cross-package: [session turn control](../../../../docs/dev/changesets/2026-11-13-session-turn-control.md). Product: [Sending a session a turn](../../../../docs/ft/daemon/project-concept.md#sending-a-session-a-turn).

- `exec_tool/project_home/turn.rs`: `impl ProjectHomeTurnPort for DaemonProjectHome` — refusals (no `turn_input`, stopped, pending question/permission), paste + submit key into the main terminal without a lease change, resume then deliver once the terminal draws (60 s). `acp_prompt` refused before any resume (`TODO(acp-turn-input)`).
- `project_home_inbox.rs`: `scope_of(home, started_by_me)`.
- Docs: [architecture § exec_tool](../architecture.md).
