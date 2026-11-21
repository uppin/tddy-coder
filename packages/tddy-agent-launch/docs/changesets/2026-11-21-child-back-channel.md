# 2026-11-21 — An orchestrator's child is told how to report back

**Type:** Feature · `#orchestra` 2/9 (PR #980) · Cross-package: [docs/dev/changesets/2026-11-21-child-back-channel.md](../../../../docs/dev/changesets/2026-11-21-child-back-channel.md) · Feature: [project concept § Reporting to the orchestrator](../../../../docs/ft/daemon/project-concept.md#reporting-to-the-orchestrator)

- `back_channel`: the channel choice (native peer, `tddy-tools send`, `TddyMessage` hook-log line),
  its guidance (`ReportsTo::guidance_at`), peer names (`peer_name_args`) and `first_prompt_with`.
- `cli_agent_steps::wire_session_agent` puts the guidance in the operator-notify system prompt, or in
  `SessionWiring.first_prompt_guidance`. It mints a hook token for a target child whose agent installs
  no hooks. A resume writes the system-prompt guidance and the peer name again
  (`LaunchSessions::resumed_reports_to`).
- `remote_hook_message` posts a `TddyMessage` line through
  `HookReportDelivery::post_message_to_orchestrator`.
- Splits: `remote_hook_delivery.rs` (from `remote_hook_follower.rs`, 542 → 386),
  `cli_agent_steps/resumed_wiring.rs` (537 → 390), `opencode_cli_spawn/start_steps.rs`
  (522 → 428). Closed `oversized-file-remote-hook-follower` (517 at detection, 386 at close).
