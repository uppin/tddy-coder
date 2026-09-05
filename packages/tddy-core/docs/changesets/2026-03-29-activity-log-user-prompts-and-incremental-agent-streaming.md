# 2026-03-29 — Activity log user prompts and incremental agent streaming

**Type:** Feature

**`presenter::activity_prompt_log`** (**`User:`** / **`Queued:`** lines), **`presenter::agent_activity`** (incremental tail + channel policy helpers), **`Presenter`** partial-row tracking and **`WorkflowEvent::AgentOutput`** handling (**`finalize_agent_line_in_activity_log`**, **`sync_agent_partial_activity_log`**, **`flush_agent_output_buffer`** dedupe). Workflow chunks broadcast via **`PresenterEvent::AgentOutput`** without duplicate **`ActivityLogged`** for the same streaming content. Feature doc: [activity-log-streaming.md](../../../../../docs/ft/coder/activity-log-streaming.md). (tddy-core)
