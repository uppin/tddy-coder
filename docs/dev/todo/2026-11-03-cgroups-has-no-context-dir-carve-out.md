# 2026-11-03 — The cgroups backend carries `SandboxPlan.context_dir` but adds no read-only bind for it

**Category:** Gap (new capability, deliberately not added)
**Source:** `#agent-tools` 10/12 (#799), `TODO(jail-agnostic)` at `packages/tddy-sandbox-cgroups/src/lib.rs`
(`spawn_plan`) and on `SandboxPlan::context_dir` in `packages/tddy-sandbox/src/builder.rs`

## What is there

`SandboxPlan.context_dir` is the directory the jail keeps read-only to the confined process. The
Seatbelt backend reads it and renders a `(deny file-write* (subpath …))` carve-out under the project
root. The cgroups backend has never had such a carve-out, and after #799 it still has none: the field
is carried on the plan and every launcher sets it, but no mount is derived from it.

## Why it was not changed in #799

Reading the field in cgroups means adding a read-only bind over the context directory — a new jail
capability, against #799's "no new runner capability". The orchestrator confirmed the scope at green.

## What to decide

Whether a Linux jail should hold the guidance files read-only inside the writable project tree, as a
macOS one does. If so, `spawn_plan` binds `plan.context_dir` read-only after the project-root bind,
with a test beside `stdio_piping_acceptance`.
