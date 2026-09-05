# 2026-03-08 — Remove validate-changes and DemoSkipped

**Type:** Feature

Removed legacy `validate-changes` goal (superseded by `evaluate`). Deleted `Goal::ValidateChanges`, `ValidateChangesOptions`, `ValidatingChanges`/`ValidateChangesComplete` states, `validate_changes()` method, `workflow/validate.rs`, `validate_allowlist()`, `validate.schema.json`, `ValidateOutput`, `parse_validate_response()`, `write_validation_report()`. Renamed shared types: `ValidateBuildResult` → `EvaluateBuildResult`, `ValidateIssue` → `EvaluateIssue`, etc. Removed `DemoSkipped` state and `skip_demo()` method; when demo is skipped, workflow goes directly from `GreenComplete` to `evaluate()`. `next_goal_for_state("DemoComplete")` → `"evaluate"`; removed `DemoSkipped` from mapping. Deleted `validate_integration.rs`, `validate_unit.rs`. (tddy-core)
