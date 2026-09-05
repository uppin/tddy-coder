# 2026-03-08 — Evaluate-Changes and Validate-Refactor Goals

**Type:** Feature

Added Goal::Evaluate for change analysis (changed files, affected tests, validity assessment) with evaluation-report.md output. Added Goal::ValidateRefactor for orchestrating validate-tests, validate-prod-ready, and analyze-clean-code subagents via Agent tool (Claude-only; CursorBackend rejects). New types: EvaluateOutput, EvaluateChangedFile, EvaluateAffectedTest, ValidateRefactorOutput. New states: Evaluating, Evaluated, ValidateRefactorComplete. New parsers: parse_evaluate_response(), parse_validate_refactor_response(). New permissions: evaluate_allowlist(), validate_refactor_allowlist(). (tddy-core)
