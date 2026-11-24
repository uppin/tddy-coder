# 2026-11-25 — Activity from an agent's own transcript; spoken hook messages from the spec; sanitised call ids

**Type:** Feature

`ActivityServiceImpl::record_transcript_line(TranscriptLine)` records the messages a followed transcript says (stamped before the hook calls it has not reached) and pairs its tool calls with the hooks' rows; `record_spoken_message` reads `activity.hook_messages` instead of matching `UserPromptSubmit` / `Stop`; `ReportAgentActivity` records every control or whitespace character of a `tool_use_id` as `_`. Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-source.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-source.md) (#1035).
