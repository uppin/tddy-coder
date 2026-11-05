# 2026-11-05 — tddy-tools finds its session without TDDY_* variables

**Type:** Feature

`#agent-tools` 15/19 ([#821](https://github.com/uppin/tddy-coder/pull/821)).

- `session_source` (`resolve_session`, `resolve_current_session`, `adopt_session`, `ResolvedSession`, `SessionSource`, `SessionNotFound::{Nowhere, UnreadablePointer}`): `--socket`, then the nearest `.tddy/session.json` at or above the cwd, then `TDDY_SOCKET`; none is an error naming all three, and an unreadable pointer is an error, never skipped.
- `submit`, `ask` and `transition` with no session fail (exit 1) instead of succeeding locally; `submit` no longer writes a `branch-review` `review.md` with no socket.
- Docs: [session resolution](../session-resolution.md). Tests: `tests/session_source_unit.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md).
