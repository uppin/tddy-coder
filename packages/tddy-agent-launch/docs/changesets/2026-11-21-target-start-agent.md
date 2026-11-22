# 2026-11-21 — A target session starts without the agent on the daemon host

**Type:** Feature · **Stack:** `#tab-hardening` 4/4 (PR #992)

A start or resume on a target reads the agent's spec with its bare command (`resolve_session_agent_for_target`), probes it on the target before the prelude and the worktree cut (`require_the_agent_on_the_target`), and needs no copy of the agent on the daemon host.

Full record: [`docs/dev/changesets/2026-11-21-target-start-agent.md`](../../../../docs/dev/changesets/2026-11-21-target-start-agent.md); feature: [`claude-cli-session.md`](../../../../docs/ft/daemon/claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs).
