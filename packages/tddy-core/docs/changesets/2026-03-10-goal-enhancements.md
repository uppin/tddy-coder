# 2026-03-10 — Goal Enhancements

**Type:** Feature

Replaced .session/.impl-session with changeset.yaml. Added initial_prompt and clarification_qa to changeset. Session entries have system_prompt_file. Plan workflow persists questions when ClarificationNeeded; pairs with answers on follow-up. Planning parser tries each structured-response block until one parses (handles system prompt before model output). (tddy-core)
