# 2026-11-14 — The `#orchestrators` sweep: rotation comments corrected; `start_session_core` extraction refused

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901).

## What changed

- **T2** — the `project_home_idle.rs` module doc and `stop_session` comment state that
  `.session.yaml` does change on `/clear`, and that change is what makes the next ensure resume
  the same conversation.

## Code issues

- `complexity-svc-start-session-core-start-session-core.md` kept open: `start_session_on_request`
  83 lines, `start_here_body` 273 lines (fn line to closing brace), both over the 60-line
  ceiling. Accepted as standing debt by developer consent, 2026-11-14, reason "restructure
  engine refused" — see `docs/dev/todo/2026-11-14-restructure-engine-value-returning-arms.md`.
