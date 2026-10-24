# 2026-10-24 — SessionAgentRecord carries its kind
**Type:** Feature

`SessionAgentRecord.kind: AgentKindRecord` (`model` | `coding_agent`, serde default `model`, omitted when `model`) and `harness: Option<String>`; a record written before kinds loads as a model entry (#721, `#coagents` 2/8).
