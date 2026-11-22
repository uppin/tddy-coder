# 2026-11-21 — The stub-binaries `ToolHost` and the session-record helpers are repeated across the tab-jail tests

**Category:** Test debt (left by `#session-liveness` 7/8, #966)
**Source:** `packages/tddy-agent-launch/src/tab_jail_agent_tests.rs`, `packages/tddy-session-lifecycle/src/test_util.rs`, `packages/tddy-session-lifecycle/tests/sandbox_tab_support/mod.rs`

`tab_jail_agent_tests.rs` defines `AHostOfStubs`, a `ToolHost` whose environment is the stubs' binary variables
and whose `PATH` is empty. `tddy-session-lifecycle::test_util` defines the same host as `AHostOfBinaries`, and
five more `ToolHost` fixtures with other answers sit in `tddy-agent-launch` (`EveryBinaryAt`, `AStubPathHost`),
`tddy-session-lifecycle` (`AFixtureHost`) and `tddy-coder` (`HostNamingBinary`, `AHostWithStubAgentTools`). The
tab-jail agent tests and the `sandbox_tab_support` fixture also each carry a `has_a_main_usage_context` /
`session_records` pair.

**Why deferred:** the obvious home, `tddy-testing-commons`, would need a new dependency on `tddy-actions` — and
`tddy-session-lifecycle::test_util` is a public module of the library (a dev-dependency is not reachable from it),
so it cannot import the shared one without making `tddy-testing-commons` a normal dependency of a production crate.
Choosing between those is a dependency decision, not a cleanup; it was tried inside #966 and backed out.

**What would close it:** move `test_util`'s fixture builders behind a `test-util` feature (or into a test-support
crate) so they may use `tddy-testing-commons`, add one `ToolHost` fixture and the session-record helpers there, and
have the seven copies import it with their assertions unchanged.

**Narrowed 2026-11-21 (`#tab-hardening` 2/4, #990):** the new `tddy-test-hosts` crate (`a_tool_host()`, no sandbox
dependencies, so `tddy-session-lifecycle::test_util` may depend on it) replaced `AHostOfStubs`, `AHostOfBinaries`,
`AStubPathHost`, `AHostWithoutAnyAgent` and every `AFixtureHost` / `AHostOf` in `tddy-session-lifecycle` and
`tddy-agent-launch`. What remains: `EveryBinaryAt` (`tab_jail_launch_tests.rs`: it answers every `*_BINARY` variable,
which `a_tool_host()` would have to enumerate), the `ToolHost` copies in other crates (`tddy-coder`,
`tddy-actions` tests, `tddy-build`, `tddy-daemon-sandbox`, `tddy-sandbox-*`, `tddy-coding-agent-specs`,
`tddy-telegram-control`; `tddy-actions` is below the new crate, so its own copies would close a cycle), and the
`has_a_main_usage_context` / `session_records` pair of the tab-jail tests.
