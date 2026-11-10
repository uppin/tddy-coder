# 2026-11-03 — A jailed tool with no `network:` section gets no egress policy, and the runner then cannot bind its shim

**Category:** Latent defect in `#agent-tools` 5/12 (#794), found by `#agent-tools` 7/12 (#796)
**Source:** `/green` on `feature/agent-tools/cursor-cli`

## What is wrong

`runner_argv` (`packages/tddy-agent-launch/src/jail_launch.rs`) passes the runner an egress policy
only when the definition lowers to `EgressPolicy::Any` or `Hosts`. A definition with **no**
`network:` section lowers to `EgressPolicy::None`, no shim port is picked and no policy flag is
passed, and the runner then defaults its policy to `any` and tries to bind a shim it was given no
port for, so the jail fails to start.

`cursor.yaml` hit it until its `network.egress.hosts` list was declared, which is why that list is
no longer commented out. Any other definition (a user's `<tddyhome>/tools/*.yaml`) that omits
`network:` and is launched jailed will hit it too.

## Fix

Either pass the runner an explicit `none` policy, or make the runner default to `none` when it is
given no policy. Not fixed in #796: the launcher is #794's.
