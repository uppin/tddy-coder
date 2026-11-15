# 2026-11-15 — CLI agent sessions list the orchestrator and recipe their changeset records

**Type:** Fix

The CLI arm of `session_list_status_from_session_dir` reads the session's
changeset once and answers its `orchestrator_session_id` and `recipe` (with
`stack_id` and `branch` from the same read), for every agent CLI type. A
session whose changeset names no orchestrator lists none.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-pairing.md](../../../../docs/dev/changesets/2026-11-15-split-session-pairing.md).
