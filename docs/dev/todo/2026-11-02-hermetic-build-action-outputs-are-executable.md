# 2026-11-02 — A hermetic build action can execute the outputs it writes

**Category:** Deferred from `#agent-tools` 4/12 (#793)
**Source:** `/validate-changes` on `feature/agent-tools/build-tools`

## What is missing

`plan_action` (`packages/tddy-daemon-sandbox/src/build_action_sandbox.rs`, `FIXME(build-tools)`)
mounts each declared output with `MountSpec::read_write`. The Seatbelt profile
(`tddy-sandbox-darwin` `profile.rs`) grants `process-exec*` on every mount, and the cgroups jail
mounts them without `MS_NOEXEC`. An action can therefore write a binary into its own output and run
it, which steps outside "only the tool closure is executable".

## Why it was deferred

`MountSpec` (`tddy-sandbox` `builder.rs`) has no write-without-exec mode; adding one changes the
shared sandbox plan and both backends, which no #agent-tools node owns. #793's acceptance criterion —
an exec of a binary outside the closure is refused — holds for every binary the action did not write.

## What would close it

A `MountSpec` flag for a writable, non-executable mount, honoured by Seatbelt (no exec rule for that
mount) and cgroups (`MS_NOEXEC`), used by `plan_action` for outputs; plus a jail test that an
action exec'ing a script it wrote into its output fails.
