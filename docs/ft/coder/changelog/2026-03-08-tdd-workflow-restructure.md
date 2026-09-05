# 2026-03-08 — TDD Workflow Restructure

- **Full workflow**: plan → acceptance-tests → red → green → demo-prompt → evaluate (previously ended at green)
- **Demo step**: Extracted from green into standalone goal; user prompted "Run demo? [r] Run [s] Skip" after green; Skip proceeds to evaluate
- **CLI rename**: `--goal evaluate` replaces `--goal validate-changes`; `--goal demo` added for standalone demo
- **Early changeset**: `changeset.yaml` written immediately after user enters prompt (before plan agent), so plan dir is resumable even if planning fails
- **Single Workflow instance**: Plain full-run uses one Workflow instance throughout (like TUI path)
- **State machine**: `DemoRunning`, `DemoComplete`; `next_goal_for_state`: GreenComplete → demo, DemoComplete → evaluate; when demo skipped, evaluate runs directly from GreenComplete
