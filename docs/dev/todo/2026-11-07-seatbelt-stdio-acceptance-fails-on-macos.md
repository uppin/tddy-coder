# 2026-11-07 — The seatbelt stdio acceptance tests fail on macOS: the jail's egress shim cannot bind

**Category:** Known failing test
**Source:** `#agent-tools` 24/28 (`dead-launch-code-removal`, #832), green stage. The two tests were
red on the parent commit `788e38432` as well, once the `SandboxHandle` import was added, so this is not
caused by the removal.

## What fails

`packages/tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs`:
- `round_trips_an_echo_over_stdio_through_a_real_seatbelt_jail`
- `dispatches_a_tool_call_through_run_host_relay_over_stdio_through_a_real_seatbelt_jail`

Both panic `sandbox child died before ready marker: sandboxed child exited with code Some(1)`. The
child's own stderr (`sandbox-exec.stderr.log` in the egress dir) shows the cause:

```
sandbox-runner failed: bind egress shim: Operation not permitted (os error 1)
```

## Why

The test picks a free loopback port and passes it to the seatbelt profile as `loopback_allow_ports`,
but its runner argv never passes `--egress-shim-port`. The runner then binds an ephemeral egress shim
port the profile does not allow, and the bind is denied. The production action runner does pass the
flag: `packages/tddy-daemon-sandbox/src/sandbox_plan_builder.rs:214`.

This is inferred from the argv recorded in `sandbox-spawn.json` (no `--egress-shim-port`) and the
denied bind. It has not been confirmed by adding the flag and re-running the tests.

## What to do

Pass the test's `shim_port` to the runner with `--egress-shim-port`, as `sandbox_plan_builder.rs` does,
then remove the two `#[ignore]` attributes and confirm both tests pass on macOS. Test-only change.

## Why it is deferred

Found while landing #832, whose scope is removal, not this harness. The two tests carry
`#[ignore]` with a reference to this record so the removal node can go green without fixing an
unrelated harness.
