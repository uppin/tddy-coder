# 2026-09-10 — `sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags` greps a file the connection-service split emptied

**Category:** Stale test, failing on `master`
**Source:** discovered by `#unbundle` node 3, [#472](https://github.com/uppin/tddy-coder/pull/472), during M3's verification

`packages/tddy-daemon/tests/sandbox_session_stdio_acceptance.rs:203` asserts on **source text**, not
behaviour:

```rust
let connection_service_rs = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/connection_service.rs"
));
assert!(connection_service_rs.contains("\"--stdio\""), "sandbox-runner spawn argv must pass --stdio");
```

PR [#468](https://github.com/uppin/tddy-coder/pull/468) (`#connection-service-split` 2/2) moved the
sandboxed-session spawn out of `connection_service.rs` into three submodules. The argv now lives at
`connection_service/svc_start_sandboxed_claude_cli_session.rs:476`,
`svc_start_sandboxed_cursor_cli_session.rs:327` and `svc_relaunch_sandboxed_runner.rs:199`. The
parent file no longer contains the literal, so the test fails — and has failed since #468 landed on
`master`. Verified: `git show <master>:packages/tddy-daemon/src/connection_service.rs | grep -c
'"--stdio"'` is `0`.

Its sibling in the same file,
`real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio`, passes — it
drives a real jail rather than reading source.

## Why it matters beyond the red

The `include_str!` is a **compile-time** dependency on a `tddy-daemon` source path, and it is the
only reason `sandbox_session_stdio_acceptance.rs` could not move to `tddy-daemon-sandbox` with the
subsystem in M3. Every other test in the file is pure sandbox.

## The fix

Point the grep at the three files that actually build the argv (or, better, assert on the argv a
spawn *produces* rather than on the text that produces it — the suite already has a real-jail test
that could carry the assertion). Then the whole file can follow the sandbox subsystem into
`tddy-daemon-sandbox`.

Not fixed in #472: the failure predates the branch, and repointing a test's subject is not a
relocation.
