# 2026-11-03 — `host_home` on the plan requests is read by no recipe

**Category:** Cleanup
**Source:** `#agent-tools` 10/12 (#799), `packages/tddy-sandbox-recipes/src/plan.rs`
(`RunnerPlanRequest::host_home`, `ProcessPlanRequest::host_home`) and
`packages/tddy-daemon-sandbox/src/sandbox_session.rs` (`SandboxRunnerSpawn::host_home`)

## What is there

`host_home` fed the per-session credential copy of the Claude and Cursor recipes. #799 deleted those
recipes — a tool's home and seeded files now come from its definition (`tool_grants::lower_files`) —
so the field is carried through `SandboxRunnerSpawn` → `RunnerPlanRequest` and read by nothing.

## Why it stays

Removing it edits every launcher and test that builds a `SandboxRunnerSpawn`, a `RunnerPlanRequest` or
a `ProcessPlanRequest` (about fifteen sites), none of which changes behaviour.

## What to decide

Drop the field in one mechanical change.
