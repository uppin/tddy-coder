# 2026-09-10 — `execute-tool-stdio-fixture` forces three dev-dependencies into `[dependencies]`

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474)

`packages/tddy-tools/Cargo.toml` declares a second binary whose only purpose is to be a test
fixture:

```toml
[[bin]]
name = "execute-tool-stdio-fixture"
path = "tests/fixtures/execute_tool_fixture.rs"
```

It hosts a fake `connection.ConnectionService/ExecuteTool` handler over its own stdin/stdout for
`tests/session_tool_stdio_rpc_dispatch.rs`. Because it is a real `[[bin]]` target, plain
`cargo build` and `cargo clippy` build it — and neither pulls in `[dev-dependencies]` — so
`tddy-rpc`, `tddy-stdio` and `async-trait` sit in `[dependencies]` where nothing in
`src/**` needs them. The manifest carries a comment saying exactly this, which is honest and still
wrong: three dependencies of the shipped library are there for a test.

Node 5 dropped 17 of `tddy-tools`' dependencies and these three survived it, so they are now a
visible share of what is left.

Two ways out, both real:

- Move the fixture into the test that uses it and have that test build it with
  `escargot`/`cargo`-driven compilation, or spawn the handler in-process on a Unix socket the way
  `mcp_stdio_dynamic_tools_acceptance.rs` already does — that suite hosts the same fake service
  from inside the test process and needs no fixture binary at all.
- Or move the fixture to a tiny `tddy-tools-test-fixtures` crate that owns the three dependencies.

The first is the better one and is mostly already written elsewhere in the same suite directory.

Deferred because it deletes or rewrites a passing test's harness, and node 5 does not touch tests to
make manifests tidier.
