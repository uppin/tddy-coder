# 2026-11-05 — The session is resolved and adopted before the runtime starts

**Type:** Feature

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `main` resolves the session (`tddy_tools::session_source::resolve_current_session`, honouring `--socket`) and adopts it into the process environment before it builds the multi-threaded runtime; an unreadable pointer exits 1 at start-up.
- Tests: `tests/session_pointer_submit.rs` (a shell-run `submit` with no `TDDY_*` reaches its session through the pointer); the no-session cases in `cli_integration.rs` and `branch_review_persist_red.rs` expect the error naming the three sources.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
