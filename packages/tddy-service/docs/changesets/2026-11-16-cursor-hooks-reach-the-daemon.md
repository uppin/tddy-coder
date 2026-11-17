# 2026-11-16 — ReportAgentActivityRequest carries the agent's tool_use_id

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

`ReportAgentActivityRequest` gains `tool_use_id` (field 10): the agent's id for the tool
call (Cursor's `tool_use_id`), pairing its pre and post reports. Empty for agents that send
none. TS bindings regenerated; the drift gate is clean.
