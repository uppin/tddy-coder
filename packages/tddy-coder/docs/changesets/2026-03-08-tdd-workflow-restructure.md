# 2026-03-08 — TDD Workflow Restructure

**Type:** Feature

Full workflow: plan → acceptance-tests → red → green → demo-prompt → evaluate. Demo extracted from green; user prompted after green. CLI: `--goal evaluate` replaces `validate-changes`; `--goal demo` added. Early changeset: changeset.yaml written before plan agent. Single Workflow instance in plain full-run. (tddy-coder)
