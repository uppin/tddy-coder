# 2026-11-06 — The session-starter port

**Type:** Feature

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- `session_starter`: `SessionStarter::start_for_operator(OperatorSessionStart)` → `OperatorStartOutcome::{Started, BranchConflict}`; `NoSessionStarter` (refuses `FAILED_PRECONDITION`); `LateBoundSessionStarter` (bound once, refuses before the bind, a second bind refused). See [extension-ports.md](../extension-ports.md#the-session-starter-port-session_starter).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).
