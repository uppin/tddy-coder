# 2026-10-22 — Harness profiles beside agent definitions

**Type:** Feature

`#provider-route` 8/9 ([#669](https://github.com/uppin/tddy-coder/pull/669)). New `harness_profile`
module (`HarnessProfile` with `deny_unknown_fields`, `load_harness_profiles`, `parse_harness_profile`,
`harness_profile_rows`, `names_a_harness`). `agent_def::load_agent_defs` classifies each file through
`read_agent_def_document` → `AgentDefDocument` and skips a profile with a debug log, not a parse
warning. See [roster and subagent runtime](../roster-and-subagent-runtime.md) and
[catalog service](../catalog-service.md).
