# 2026-11-13 — Orchestrators drive a session's turns

**Type:** Feature

`#orchestrators 4/11` (PR [#881](https://github.com/uppin/tddy-coder/pull/881)). Cross-package: [session turn control](../../../../docs/dev/changesets/2026-11-13-session-turn-control.md). Product: [Sending a session a turn](../../../../docs/ft/daemon/project-concept.md#sending-a-session-a-turn).

- `project_home_turn_tools`: `SendSessionMessage` (empty text refused before the port), `ResumeSession` (empty message is none), `ProjectHomeTurnPort` with refusing defaults, `TurnDelivery`, `ResumeOutcome`; in `is_project_home_tool`, the catalogue and `is_project_home_target_tool`.
- `WaitProjectNotifications` / `ReadProjectNotifications`: `started_by_me` (boolean, default false) passed to `ProjectHomeInboxPort`.
- `terminal_keys` reads `named_key_bytes` from `tddy-coding-agent-specs` (new dependency).
- `src/lib.rs` oversized-file record: 889 → 890.
