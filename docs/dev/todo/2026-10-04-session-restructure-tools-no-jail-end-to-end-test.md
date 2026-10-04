# 2026-10-04 — no test covers a session's restructure tools reaching the host from a jail

**Category:** Deferred test
**Source:** `#live-plan` 14/15, [#573](https://github.com/uppin/tddy-coder/pull/573), `/pr-wrap` wrap gate; deferred with the developer's agreement.

## What is left

The `restructure_*` tools are tested at the host executor (`tddy-lsp-executor`, 12 tests against a fake
`code_index` over a socket) and at the advertisement gate (`mcp_tool_advertisement_audit`). Two pieces
that connect them are verified by compile and clippy only:

- the daemon's registration of `IndexRestructureExecutor` when `index_daemon:` is configured
  (`packages/tddy-daemon/src/runtime.rs`, beside the `Lsp*` registration);
- the `TDDY_RESTRUCTURE_TOOLS` export (`restructure_tools_env` in `jail_env_builders.rs`), set only when
  an executor is registered.

Nothing asserts that a call made inside a jail reaches the host executor, or that without `index_daemon:`
the tools are not advertised to the session.

## Why it was left

An in-jail end-to-end test needs a sandboxed session and a daemon with a managed index. The `Lsp*` tools
have the same gap, and the macOS sandbox acceptance suite is a known harness gap, so the test is a node
of its own, not a side effect of this PR.

## What would close it

A test that builds the daemon with and without `index_daemon:` and asserts the jail env carries
`TDDY_RESTRUCTURE_TOOLS` only in the first case, plus one `restructure_check` call relayed from a jail to
the host executor. Worth writing together with the equivalent for the `Lsp*` tools.
