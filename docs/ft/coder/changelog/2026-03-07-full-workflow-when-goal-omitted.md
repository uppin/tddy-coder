# 2026-03-07 — Full Workflow When --goal Omitted

- **Full workflow**: When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green in a single invocation
- **Resume**: Auto-detects completed state from `changeset.yaml`; re-running skips completed steps (via `--session-dir`)
- **CLI**: `--goal` is now optional; individual goals (`plan`, `acceptance-tests`, `red`, `green`) unchanged
- **Output**: Full workflow prints green step output on success; when `GreenComplete`, re-running exits with summary
