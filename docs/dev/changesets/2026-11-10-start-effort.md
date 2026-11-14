# 2026-11-10 — Effort on session start

**Type:** Feature · `#orchestrators` 3/11 ([#880](https://github.com/uppin/tddy-coder/pull/880))

A session can be started at a reasoning effort level as well as a model. The agent's spec declares the levels and the carry (`coding_agent.effort`, a flag or an env variable); claude's maps it to `--effort`. The start refuses a level the agent does not declare; the level is recorded in `.session.yaml` and re-applied on resume, on the host and in a jail. The exec `StartSession` takes `effort`, `DescribeSessionStartOptions` and `ListAgents` report the levels, and the New-session form shows an Effort picker.

- Developer decision (2026-11-12): jailed sessions carry the effort too, through the same spec-driven `effort_launch`; before it a sandboxed start accepted and dropped one.
- `HarnessLaunch` gained no `effort` field; the carry rides the slot argv and the launch env.

Packages: [tddy-service](../../../packages/tddy-service/docs/changesets/2026-11-10-start-effort.md), [tddy-coding-agent-specs](../../../packages/tddy-coding-agent-specs/docs/changesets/2026-11-10-start-effort.md), [tddy-agent-launch](../../../packages/tddy-agent-launch/docs/changesets/2026-11-10-start-effort.md), [tddy-changeset](../../../packages/tddy-changeset/docs/changesets/2026-11-10-start-effort.md), [tddy-tool-engine](../../../packages/tddy-tool-engine/docs/changesets/2026-11-10-start-effort.md), [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/docs/changesets/2026-11-10-start-effort.md), [tddy-session-create-web](../../../packages/tddy-session-create-web/docs/changesets/2026-11-10-start-effort.md). Product: [Claude CLI session § Effort](../../ft/daemon/claude-cli-session.md#effort).
