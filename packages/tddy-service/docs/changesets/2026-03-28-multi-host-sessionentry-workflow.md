# 2026-03-28 — Multi-host + SessionEntry workflow

**Type:** Feature

`connection.proto`: `ListEligibleDaemons` RPC; `EligibleDaemonEntry`; `daemon_instance_id` on `StartSessionRequest` (field 5) and `SessionEntry` (field 8); `workflow_goal`, `workflow_state`, `elapsed_display`, `agent`, `model` on `SessionEntry` (fields 9–13); `recipe` on `StartSessionRequest` where applicable. (tddy-service, tddy-web codegen)
