# 2026-10-04 — `#keyring` 5/9 grew `project/coordinate_handlers.rs` past the 500-line budget

**Category:** Deferred decomposition
**Source:** `#keyring` 5/9, [#512](https://github.com/uppin/tddy-coder/pull/512) — `/pr-wrap` step 3.5
file-length gate. Deferred **with the developer's explicit consent** (asked and answered during that
`/pr-wrap` run), not silently.

## What happened

`packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs` went **528 → 650** production lines in this
PR: `set_project_accounts_at_project_coordinate` (~120 lines) was added beside its siblings, as
`set_project_default_branch_at_project_coordinate` had been. The file was already over budget (528 at
`4e7157d2`), so this is growth, not a new crossing.

## Why it was not split in the PR

The developer chose to defer. No other `#keyring` PR touches this file (#513–#516 were checked), so the
stack constraint did not apply — this is a cost/benefit call, not a conflict.

## What would close it

The seams are already designed in
[`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`](../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md):
A `add_project_to_host_at_project_coordinate` (~175), B the branch operations — which now include
`set_project_accounts_at_project_coordinate` (~285 with it), C list/create (~160). Prove them with
`restructure check --deep`, apply with the `code-restructuring` skill, one `tddy-daemon-rpc` test run at
the end. Seam B is now the largest.

Also see the same handler's near-duplicate of the repeated-provider refusal in
`tddy-projects::project_storage::set_project_accounts` — a split is the moment to decide which of the two
owns it.

## Also deferred with it: the handler prelude

`set_project_accounts_at_project_coordinate` is **114 lines** and `set_project_default_branch_at_project_coordinate`
is **101** (limit 60). Both open with the same ~50-line authenticate → OS user → `project_id` → classify
route → forward prelude. Extract one `authenticate_and_route` returning a local-or-forward decision and
use it from both — do it in the same change as the split above, since both rewrite the same file.

