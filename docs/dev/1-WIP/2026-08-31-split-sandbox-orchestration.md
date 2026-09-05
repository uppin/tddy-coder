# Changeset: split-sandbox-orchestration

State A → State B amendment: [`docs/ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md`](../../ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md)
PRD (target): [`docs/ft/daemon/remote-managed-worktree.md`](../../ft/daemon/remote-managed-worktree.md)

## Responsibility

- Remove split+sandbox refusal in `start_split_claude_cli_session`.
- End-to-end split start with `sandbox = true`: agent unsandboxed on A, codebase sandboxed on B via forwarded workspace request.
- Cross-host acceptance proving confinement on B and unsandboxed agent on A.
- PRD update to `remote-managed-worktree.md` for inverted split sandbox semantics.

## Boundaries

- Does **not** implement the workspace sandbox jail itself — consumes `workspace-tool-sandbox` (`run_exec_tool_locally` dispatch + workspace metadata + jail provision), which landed in #427.
- Does **not** implement resume/relaunch of sandbox after stop — `split-sandbox-resume` owns that.
- Does **not** change web form or Cypress — `web-split-sandbox-toggle` exposes the checkbox.
- Does **not** change co-located sandbox or allow `recipe` on split.

## Dependencies

- **`workspace-tool-sandbox`** (#427, landed): delivers sandboxed workspace `ExecuteTool`/`StreamExecuteTool` when workspace metadata has `sandbox: Some(true)`, plus workspace start accepting and persisting the flag. This PR must not build a second jail or duplicate `run_exec_tool_locally` sandbox routing — it only stops refusing the combination and forwards the existing field.

## Draft PR contract

Land first:

1. Delete `req.sandbox` invalid_argument block in `start_split_claude_cli_session` (failing cross-host test: split+sandbox start currently rejected).
2. Assert workspace half metadata `sandbox: Some(true)` and agent half `sandbox: None` after successful split+sandbox start (failing metadata test).
3. One cross-host `ExecuteTool` confinement assertion on codebase daemon B (depends on parent jail being present on the branch).

`split-sandbox-resume` and `web-split-sandbox-toggle` branch off this ref once the refusal is gone and the happy-path start test exists.

## TODO

- [x] Create/update PRD documentation — amendment at `docs/ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md`
- [x] Create changeset — this document
- [x] Failing acceptance tests (Step 6 — `/plan-red`)
- [x] Failing unit/integration tests (Step 7 — red phase)
- [x] Implement production code making tests pass (`/green`)
- [x] Apply amendment to `remote-managed-worktree.md` (`/wrap-context-docs`)
- [x] Prepend changeset index line to `docs/dev/changesets/` (`/wrap-context-docs`)

## Validation Results

- `cargo build -p tddy-daemon` — pass.
- `cargo test -p tddy-daemon --lib workspace_start_request_unit_tests::` — 11/11 pass (forward-contract regression guards stay green).
- `remote_managed_worktree_acceptance.rs::a_split_start_asking_for_a_sandbox_is_admitted_and_fails_over_the_missing_room` — pass (flipped from `InvalidArgument` refusal to `FailedPrecondition` missing room).
- `remote_managed_worktree_cross_host_acceptance.rs` — compiles; runtime blocked by LiveKit testkit WebRTC signalling timeout affecting the whole cross-host suite in this environment (not specific to these tests). To verify on CI / a host with a healthy LiveKit.
- `cargo clippy -p tddy-daemon -- -D warnings` — pass.

## Status

Implementation complete. Ready for wrap: this WIP source is removed after the PR lands.
