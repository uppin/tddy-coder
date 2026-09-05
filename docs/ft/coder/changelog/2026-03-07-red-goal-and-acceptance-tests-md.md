# 2026-03-07 — Red Goal & Acceptance-Tests.md

- **Red goal**: `--goal red --session-dir <path>` reads PRD.md and acceptance-tests.md, creates skeleton production code and failing lower-level tests via Claude
- **acceptance-tests.md**: acceptance-tests goal now writes acceptance-tests.md (structured list + rich descriptions) to the session directory
- **State machine**: New states RedTesting, RedTestsReady
- **CLI**: `--goal red` requires `--session-dir`
