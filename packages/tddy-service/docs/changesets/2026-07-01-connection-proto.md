# 2026-07-01 — **`connection.proto`

**Type:** Feature

`stack_plan_json` field on `SessionEntry`** — `string stack_plan_json = 23`; JSON-serialized `Stack` for `pr-stack` orchestrator sessions, empty string until a plan exists. Powers the web PR-Stack Chat Screen's planned-PR list. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md#pr-stack-chat-screen). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
