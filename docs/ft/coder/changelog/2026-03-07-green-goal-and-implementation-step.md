# 2026-03-07 — Green Goal & Implementation Step

- **Green goal**: `--goal green --session-dir <path>` resumes red session via `.impl-session`, implements production code to make failing tests pass, updates progress.md and acceptance-tests.md
- **Red goal**: Now persists session ID to `.impl-session` for green to resume
- **State machine**: New states GreenImplementing, GreenComplete
- **Documentation**: Red and green moved to `implementation-step.md`; `planning-step.md` covers only plan and acceptance-tests
- **CLI**: `--goal green` requires `--session-dir`
