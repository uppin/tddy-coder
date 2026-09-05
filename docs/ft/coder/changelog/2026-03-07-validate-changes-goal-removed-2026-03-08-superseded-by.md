# 2026-03-07 — Validate-Changes Goal (removed 2026-03-08, superseded by evaluate)

- **New goal**: `--goal validate-changes` analyzed current git changes for risks (build validity, test infrastructure, production code quality, security). Produced validation-report.md in working directory.
- **Standalone**: Callable from Init without prior plan/red/green. Optional `--session-dir` for changeset/PRD context. Used a fresh session (not resumed).
- **Permission**: validate_allowlist permitted Read, Glob, Grep, SemanticSearch, git diff/log, find, cargo build/check.
- **State**: Init → Validating → Validated. Not in next_goal_for_state auto-sequence.
- **CLI**: `--conversation-output <path>` writes raw agent bytes in real time (each line appended as received).
