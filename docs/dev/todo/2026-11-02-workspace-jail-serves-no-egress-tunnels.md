# 2026-11-02 — The workspace tool jail serves no egress tunnels, so a declared host is unreachable there

**Category:** Deferred from `#agent-tools` 3/12 (#792)
**Source:** `/validate-changes` on `feature/agent-tools/egress-allowlist`

## What is missing

`build_workspace_tool_plan` (`packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`) now
starts the runner's egress shim when a jail tool declares `hosts` or `any`, and the shim enforces the
policy. But the daemon side of that jail does not run `tddy_sandbox_runner::run_host_relay`: it
drives the `SessionChannel` itself through `exchange_in_jail_tool_call`, which skips every frame that
is not an `InJailToolResponse`. A `TunnelOpen` from the shim is therefore never answered, and the
shim returns `502` once its wait times out.

So in production a workspace-jail tool can be **refused** a host (shim `403`) but never **reach** a
declared one. This fails closed, so it is not a confinement hole. Today the only workspace-jail tool
is `rg` (`WORKSPACE_JAIL_TOOLS`), which declares no network, so no shipped tool is affected. A user's
`<tddyhome>/tools/rg.yaml` that declares hosts would hit it.

The relay-side check (`HostRelayConfig::with_egress`) is also unused in production for the same
reason. Agent jails run `run_host_relay` with `EgressGuard::unrestricted` until #794/#796/#797 pass
their lowered policy.

## Why it was deferred

#792's responsibility was the policy, its lowering and the two enforcement points. The acceptance
test proves both points with the real runner and the real relay in-process. Giving the workspace jail
a host-side tunnel server changes what that jail can do, beyond enforcing a policy, and no node in
`#agent-tools` owns it.

## What would close it

Answer `TunnelOpen` / `TunnelData` / `TunnelClose` on the workspace jail's channel with the host
relay's tunnel half, configured with `EgressGuard::new(<lowered policy>, session_id, <tool ids>)`.
Add an acceptance test where a workspace-jail tool declaring a local host reaches it.
