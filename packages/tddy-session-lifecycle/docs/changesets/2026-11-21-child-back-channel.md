# 2026-11-21 — A target child's report reaches its orchestrator

**Type:** Feature · `#orchestra` 2/9 (PR #980) · Cross-package: [docs/dev/changesets/2026-11-21-child-back-channel.md](../../../../docs/dev/changesets/2026-11-21-child-back-channel.md) · Feature: [project concept § Reporting to the orchestrator](../../../../docs/ft/daemon/project-concept.md#reporting-to-the-orchestrator)

`svc_activity_ports/orchestrator_message.rs`: `post_remote_message_to_orchestrator` publishes a
target child's `TddyMessage` as the `message` record `PostSessionMessage` writes, to the
orchestrator its changeset names on this daemon (`local_orchestrator_of`). `ActivityHookDelivery`
serves it. Tests: `tests/child_back_channel_launch_acceptance.rs`.
