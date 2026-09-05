# 2026-03-07 — Full Workflow When --goal Omitted

**Type:** Feature

Made --goal optional; omitting it runs plan → acceptance-tests → red → green with auto-resume from changeset.yaml state. Added next_goal_for_state(), run_full_workflow(). (tddy-coder, tddy-core)
