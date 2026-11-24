# 2026-11-25 — AgentActivityHub pairs a transcript's tool calls with its hooks'

**Type:** Feature

`tool_turns::ToolTurns` (`AgentActivityHub::tool_turns`): the Nth call of a tool one source reports in a turn pairs with the other's Nth, a hook call adopts a transcript call's id, turns aligned on the hooks' `Done` and the transcript's `turn_end`, `earliest_unreached_hook_call` for ordering; `first_time_spoken` dedupes a message both sources report. Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-source.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-source.md) (#1035).
