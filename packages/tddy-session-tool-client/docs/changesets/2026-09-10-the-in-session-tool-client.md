# 2026-09-10 — The in-session tool client

**Type:** Architecture

New crate, added by node 5 of the `#unbundle` stack
([#474](https://github.com/uppin/tddy-coder/pull/474)). Full story in the cross-package entry:
[2026-09-10-unbundle-tools-thinning.md](../../../../docs/dev/changesets/2026-09-10-unbundle-tools-thinning.md).

`session_tool_client` moved verbatim out of `tddy-tools`, 1,005 non-blank lines at the crate root:
`SessionToolTransport`, `detect_session_tool_transport`, `dispatch_session_tool` and the
per-transport dispatchers. See the [README](../../README.md).

**It is its own crate because `tddy-service` is a Cargo cycle.** Every message it sends is a
`tddy-service` proto, which is where the plan put it. `dispatch_session_tool` selects between all
transports in one place and its LiveKit arm needs `tddy-livekit`, which has `tddy-service` in
`[dependencies]` — so the edge yields `error: cyclic package dependency: package tddy-service
depends on itself`, and `optional = true` behind a feature does not exempt it. Proven rather than
argued. Three ways out were rejected first: splitting the selector duplicates the transport
decision; injecting the LiveKit connector at runtime reintroduces exactly the function-pointer
transport the same node deleted from `tddy-bsp`; repointing the daemon and darwin suites at
`dispatch_via_sandbox_ipc` deletes their coverage of env-driven transport detection to make a
manifest assertion pass.

Sitting **above** both `tddy-service` and `tddy-livekit` also discharges a debt the plan had recorded
against the original destination: `tddy-service` depends on `tddy-tui`, so a client hosted there
would have pulled the TUI into every in-jail binary that dispatches a tool call.

**`livekit` is off by default**, unlike `tddy-tools`' own. `tddy-sandbox-app` and
`tddy-sandbox-darwin` only ever dispatch over the in-jail socket and now link no LiveKit SDK to do
it; `tddy-tools` and `tddy-daemon` opt in explicitly, so the shipped default is unchanged. Without
the feature `dispatch_via_livekit` still exists and reports the build-time omission rather than
degrading to a transport aimed at the wrong host.

**Every failure is a `{"error": …, "is_error": true}` JSON string, not a typed error** — the caller
hands it straight to a model as the tool's answer. The draft PR's declared `SessionToolTransport::
from_env()`, `SessionToolError` enum and typed `Result` were invented and do not exist; the real
surface is a free detection function and a `String`-returning dispatch.

`tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin` reach the daemon through this crate and
**no longer depend on `tddy-tools` at all** — asserted by an `unbundle_tools_dependency_dropped` test
in each, because a dev-dependency survives invisibly.

The log target moved with the code: `tddy_tools::session_tool_client` is now
`tddy_session_tool_client`. An operator watching the worktree-activity stream wants
`RUST_LOG=tddy_session_tool_client=debug`.

Nothing new entered the workspace — all six dependencies were already `tddy-tools` dependencies.

Tests: 5 / 0, plus end-to-end coverage through `tddy-daemon`'s `sandboxed_claude_cli_acceptance` and
`tddy-sandbox-darwin`'s `sandbox_runner_acceptance`.
