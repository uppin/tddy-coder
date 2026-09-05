# 2026-08-29 — agent session status inference

**Type:** Feature

`session_agent_inference.rs` infers what a claude-cli / cursor session's agent is doing by tailing its resolved ACP transcript once and its `AgentActivityHub` broadcast thereafter, and `ListSessions` reports it as `SessionEntry.agent_status` / `last_activity` in the roster's own vocabulary. One mapper for live rows and replayed frames; subscribe before seeding; nothing persisted; only `claude-cli` and `cursor-cli` tailed. See [agent-session-status.md](../agent-session-status.md).
