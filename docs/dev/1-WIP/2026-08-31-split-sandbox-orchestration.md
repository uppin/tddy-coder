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
- [ ] Failing acceptance tests (Step 6 — `/plan-red`)
- [ ] Failing unit/integration tests (Step 7 — red phase)
- [ ] Implement production code making tests pass (`/green`)
- [ ] Apply amendment to `remote-managed-worktree.md` (`/wrap-context-docs`)
- [ ] Prepend changeset index line to `docs/dev/changesets.md` (`/wrap-context-docs`)
