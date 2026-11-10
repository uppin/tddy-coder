# 2026-11-02 — CI runs no Linux jail: every cgroups jail suite self-skips there

**Category:** Deferred from `#agent-tools` 4/12 (#793)
**Source:** `/fix-pr` on `feature/agent-tools/build-tools` (CI run 41478190972)
**Consent:** the developer chose to defer this on 2026-11-02 during `#agent-tools` 4/12.

## What is missing

CI's `ubuntu-*` runners restrict unprivileged user namespaces (AppArmor
`kernel.apparmor_restrict_unprivileged_userns`), so `tddy_sandbox_cgroups` cannot create a jail
there and fails fast (`sandbox unsupported on linux: unprivileged user namespaces are unavailable`).
Every Linux jail suite therefore self-skips on CI:

- `packages/tddy-daemon-sandbox/tests/hermetic_build_action_acceptance.rs` (the two Linux-run tests)
- `packages/tddy-daemon-sandbox/tests/action_sandbox_acceptance.rs`
- `packages/tddy-sandbox-cgroups/tests/jail_smoke.rs` and `stdio_piping_acceptance.rs`

No Linux jail is exercised on CI; a skipped test reports as passing.

## What would close it

Run `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` in the Rust test jobs before
`cargo test`, so the jail suites run. **Risk:** suites that have only ever skipped on CI may surface
their own failures the first time they run; budget for fixing them in the same change.
