# 2026-11-02 — A hermetic build action cannot reach a host its tools declare

**Category:** Deferred from `#agent-tools` 4/12 (#793)
**Source:** `/validate-changes` on `feature/agent-tools/build-tools`

## What is missing

`refuse_hosts` (`packages/tddy-daemon-sandbox/src/build_action_sandbox.rs`, `TODO(build-tools)`)
refuses, with `BuildError::Exec`, any hermetic action whose tool closure declares `hosts` or `any`
egress: a build action's jail runs no egress shim, so a declared host would be silently unreachable.
Closures with no network run, with no network. This fails closed.

## Why it was deferred

#793's Responsibility is that a hermetic action's network is `lower_network` of its closure; with no
shim only the `None` policy is enforceable. Serving tunnels needs the runner's egress shim and a host
relay for build jails — the same missing piece as
[2026-11-02-workspace-jail-serves-no-egress-tunnels.md](2026-11-02-workspace-jail-serves-no-egress-tunnels.md).

## What would close it

Start the runner's egress shim in the action jail and answer its `TunnelOpen` frames under the
closure's `EgressPolicy`; then replace `refuse_hosts` with a test that a declared host is reached and
an undeclared one refused.
