# 2026-03-07 — Acceptance Tests Goal

- **New goal**: `--goal acceptance-tests --session-dir <path>` reads a completed plan, resumes the Claude session, creates failing acceptance tests, and verifies they fail
- **Session persistence**: Plan goal now writes `.session` file for session resumption
- **Testing Plan in PRD**: Plan system prompt requires a Testing Plan section (test level, acceptance tests list, target files, assertions)
- **State machine**: New states `AcceptanceTesting` and `AcceptanceTestsReady`
- **CLI**: `--session-dir` flag required for acceptance-tests goal
