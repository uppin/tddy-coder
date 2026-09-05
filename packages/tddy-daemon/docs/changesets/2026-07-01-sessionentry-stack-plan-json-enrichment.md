# 2026-07-01 — `SessionEntry.stack_plan_json` enrichment

**Type:** Feature

`session_list_status_from_session_dir` serializes `Changeset.stack` to JSON (empty string when absent) via `stack_plan_json_for_changeset`; threaded through `SessionListStatusDisplay` into proto field 23. Powers the web PR-Stack Chat Screen's planned-PR list. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md#pr-stack-chat-screen). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-service)
