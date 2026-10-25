# 2026-10-24 — Headless coding agents open as headless conversations

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). A coding agent whose harness is headless opens through `headless_command_for` and `CodingAgentSession::start_headless`; its `ParkedPermissions` are registered in the host's `ApprovalRoutes`. `AgentRoster::conversation_closed` (called on cancel and on session close) removes the route and `<tddy_data_dir>/headless/<conversation>/`. Roster entries carry `transport` and `permission_policy`. Test: `headless_coding_agent_opens_as_a_headless_conversation`.

Code issues: `oversized-file-service` 1,203 → 1,209, deferred to the sweep node.
