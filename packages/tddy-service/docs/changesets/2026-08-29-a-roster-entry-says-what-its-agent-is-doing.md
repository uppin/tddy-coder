# 2026-08-29 — a roster entry says what its agent is doing

**Type:** Feature

`SessionAgentEntry` gains `status` (`SessionAgentStatus`: UNSPECIFIED | IDLE | RUNNING | EXECUTING_TOOL | WAITING_FOR_INPUT | CONNECTING | ERROR) and `last_activity` (`SessionAgentActivity { at_unix_ms, summary }`), both riding the existing whole-snapshot `StreamSessionAgents` rather than a status read RPC — a reader that rebuilt its registry from a snapshot and then had to correlate a second stream could show a status for a row it no longer holds. `UNSPECIFIED` is documented as "this daemon has nothing to say", never "idle". One write RPC is added, `ReportAgentConversationState`, because an agent the jail was seeded with runs its loop in the jail where the daemon is never asked to open anything; it accepts only the four conversation states, since CONNECTING/ERROR describe the checkout the daemon measures itself.
