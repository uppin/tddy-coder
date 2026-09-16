# 2026-09-16 — Restructure apply progress on stderr

**Type:** Enhancement

`restructure apply`, `check`, and `anchors` now emit stamped `progress` and `indexing` lines on
stderr (elapsed time since the previous line). rust-analyzer warm-up, assist waits, and per-operation
resolve/apply steps are visible during multi-minute runs. stdout stays machine-readable for anchors
JSON and check finding lines. (tddy-code-restructuring)
