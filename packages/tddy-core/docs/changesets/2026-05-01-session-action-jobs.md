# 2026-05-01 — Session action jobs

**Type:** Feature

**`session_action_jobs`**: **`invoke_session_action`** (blocking parity with **`invoke-action`**, **`async_start`** with UUID **`job_id`**, log paths), **`wait_session_action_job`**, **`stop_session_action_job`** (Unix process-group kill), **`SessionActionJobRegistry::load`**, **`resolve_action_manifest_path`** in **`session_actions`**; **`finalize_invocation_record`** for **`test_summary`**. Feature doc: [session-actions.md](../../../../../docs/ft/coder/session-actions.md); architecture: [architecture.md](../../architecture.md#session-action-jobs-session_action_jobs). Tests: **`toolcall_jobs`**; **`session_action_jobs_acceptance`** (**tddy-tools**). (tddy-core, tddy-tools)
