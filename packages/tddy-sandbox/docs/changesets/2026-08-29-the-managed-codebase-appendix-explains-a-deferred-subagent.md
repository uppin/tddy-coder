# 2026-08-29 — the managed-codebase appendix explains a deferred subagent turn

**Type:** Feature

`sandbox_remote_appendix` now tells the agent that a `subagent_prompt` still running after its grace period answers `{responseId, pending: true}` rather than an outcome, and that `mcp__tddy-tools__subagent_await` collects it under that id — a receipt the agent cannot read as an error. Rendered only where the appendix already names the delegation tools, so a session with no replacing subagent is unchanged. Feature [managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md) § Long turns. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox)
