# 2026-06-26 — **PR-stack session UI

**Type:** Feature

connection.proto additions** — `SessionEntry` gains `orchestrator_session_id` (string, field 21): identifies child sessions belonging to a PR-stack orchestrator; `StartSessionRequest` gains `stack_parent` (string, field 15): back-reference to the orchestrating session for child spawning. TypeScript codegen regenerated. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
