# 2026-11-10 — `coding_agent.usage` replaces `transcript_usage`

`CodingAgentSection` gains `usage: Option<tddy_usage::UsageSection>`, validated at load through
`tddy_usage::validate_usage_section` from `parse_section` (no `spec/usage.rs` wrapper); a refusal
names the file. `transcript_usage` is removed and an override carrying it is refused. `claude.yaml`
and `claude-acp.yaml` bind Claude's transcripts (dedupe by `message.id`, cache counters, recursive
subagents). `#usage-binding 1/5`, PR [#890](https://github.com/uppin/tddy-coder/pull/890). See
[coding-agent-specs.md § Usage binding](../coding-agent-specs.md#usage-binding-coding_agentusage).
