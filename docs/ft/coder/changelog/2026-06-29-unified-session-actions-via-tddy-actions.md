# 2026-06-29 — Unified session actions via tddy-actions

- Session-action async jobs register in a per-session `TaskRegistry` via `ProcessRuntime` / `PipelineRuntime`; `job_id == task_id`; log paths under `session_action_jobs/jobs/<id>/` preserved
- `ActionManifest` maps to `ActionSpec` + `SessionActionExtras`; daemon exposes the same kinds through `actions.ActionService`
- Sandbox builder recipes moved to `tddy-sandbox-recipes`; daemon `sandbox_plan_builder` assembles action sandbox plans from `ActionSpec` metadata
- Feature: [session-actions.md](../session-actions.md), [sandbox-builder.md](../sandbox-builder.md)
