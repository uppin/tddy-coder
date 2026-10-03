# 2026-09-25 — the pre-apply compile gate of a restructure plan checks only the packages owning the plan's files

**Category:** Future enhancement (engine defect)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02b-task-action-services-to-daemon-sandbox.jsonl`,
ops 2 and 3 (`tests/action_service_acceptance.rs` and `tests/action_sandbox_acceptance.rs` →
`tddy-daemon-sandbox`)

## What remains

The test-binary move seeing through a glob facade, and the one-facade-per-operation lint failure that
travelled with it, are fixed (`#live-plan` 4/7, [#541](https://github.com/uppin/tddy-coder/pull/541)).
What is left of this entry:

- **The pre-apply compile gate checks only the packages owning the plan's files.** That means the
  origin, not the destination. `tddy-daemon-sandbox`'s `tests/sandbox_stdio_seatbelt_acceptance.rs`
  (`#![cfg(target_os = "macos")]`) did not compile on HEAD `d9f8f7b3` (`E0425`, `SandboxHandle` not
  imported). The pre-gate did not see it, and the post-apply gate then reported it among the plan's
  errors.

## What would fix it

Include the destination package of every cross-crate move in the pre-apply gate's set.
