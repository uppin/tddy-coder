# Changeset: PR stacks live in the project, not in a session

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Architecture Change
**Complexity**: 5 — a data migration, a new concurrency rule (per-stack lock), ~60 call sites across 8 crates
**Stack**: `#orchestrators` 2/11 · branch `feature/orchestrators/project-stack-store` · base `feature/orchestrators/orchestrator-role` (#878)

## Initial Discovery

[2026-11-10-project-stack-store-initial-discovery.md](./2026-11-10-project-stack-store-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `squatting-pr-stack-data-model.md` — [`tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md`](../../../packages/tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md)

The store must not land in `tddy-workflow-recipes`, which that record says already squats on the data
model. Not claimed.

### ⚠ DURING — `pr_stack/ports.rs` is 808 lines — [`oversized-file-pr-stack-ports.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

Call-site swaps only; no new logic in `ports.rs`.

### ⚠ DURING — `tddy-tools` `server.rs` — [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md), [`2026-10-08-tddy-tools-server-rs-…`](../todo/2026-10-08-tddy-tools-server-rs-is-3-9k-lines-and-the-crate-mixes-several-responsibilities.md)

The six `server.rs` sites swap to the store and nothing more; n7/n10 move the tools out of the file.

### ⚠ DURING — `stack-progress.json` documented but unwritten — [`2026-08-29-stack-progress-json-…`](../todo/2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md)

The wrap must not carry that claim into the store's documentation. Not claimed.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10), inherited through n1.

## Affected Packages

- [tddy-changeset](../../../packages/tddy-changeset/README.md) — `ProjectStackStore`, `Changeset.stack_id`
- [tddy-pr-stack](../../../packages/tddy-pr-stack/README.md) — `stack_ops` take a store handle
- [tddy-workflow-recipes](../../../packages/tddy-workflow-recipes/README.md) — recipe reads/writes, reseed, merge/repoint bridges
- [tddy-tools](../../../packages/tddy-tools/README.md) — six `pr_*` sites
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — PrStackService handlers
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — child spawn handler, node link
- [tddy-session-worktree](../../../packages/tddy-session-worktree/README.md) — chain base, orchestrator detection
- [tddy-session-activity](../../../packages/tddy-session-activity/README.md) — `stack_plan_json` enrichment
- [tddy-daemon](../../../packages/tddy-daemon/) — one call: the startup sweep

## Related Feature Documentation

- PRD: [PRD-2026-11-10-project-stack-store.md](../../ft/coder/1-WIP/PRD-2026-11-10-project-stack-store.md)
- [PR stacking](../../ft/coder/pr-stacking.md), [PR-Stack live status](../../ft/coder/pr-stack-live-status.md)

## Summary

A stack becomes a project record: `<projects dir>/<project_id>/stacks/<stack_id>.yaml`, written under a
per-stack lock. `Changeset.stack` is replaced by `Changeset.stack_id`. Every reader and writer goes
through `ProjectStackStore`. A daemon startup sweep migrates every existing `changeset.stack`.
Behaviour is otherwise unchanged.

## Background

The stack is owned by the pr-stack session that planned it, so nothing else can address it. The
developer decided (2026-11-10) that stacks are project-level, per host, addressed by id, and migrated by
a startup sweep with no dual-read period.

## Responsibility

- `ProjectStackStore` (`create`, `read`, `update`, `list`) and `StackRecord { stack_id, project_id,
  created_by_session_id, created_at_unix_ms, stack: Stack }` — **owned here**.
- `Changeset.stack_id: Option<String>`; removal of `Changeset.stack` from the model (a tombstone so a
  stale field is refused by the sweep, never silently read).
- Every existing reader/writer moved to the store; the startup migration sweep.

## Boundaries

- Does **not** add exec tools, `stack_id` arguments on any tool, or a way for a non-pr-stack session to
  create a stack (n7).
- Does **not** change PrStackService request shapes or the web (n9).
- Does **not** move git operations (n10).
- The stack-op journal, per-node docs, `stack-plan.yaml`, `stack-status.*` stay in the session dir.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `orchestrator-role` (#878) | `is_orchestrator_role`, the `orchestrator` role | nothing — n1 is a line predecessor only | read or change any role code |

Verified at the contract (2026-11-12): no test of this node reaches a `TODO(orchestrator-role)` stub;
every failure is this node's own `TODO(project-stack-store)`. One new **dev**-dependency edge:
`tddy-session-activity` → `tddy-pr-stack` (its acceptance test runs the sweep); `tddy-pr-stack` depends on
nothing that depends on `tddy-session-activity`.

## Draft PR contract

- `tddy_changeset::project_stack_store` (`packages/tddy-changeset/src/project_stack_store.rs`), reached
  as `tddy_core::project_stack_store` too:
  - `ProjectStackStore::new(projects_dir)` and `projects_dir()` (implemented — a constructor and accessor);
  - `create(&self, project_id, created_by_session_id, stack: Stack) -> Result<StackRecord, StackStoreError>`;
  - `read(&self, project_id, stack_id) -> Result<StackRecord, StackStoreError>`;
  - `update(&self, project_id, stack_id, f: impl FnOnce(&mut Stack)) -> Result<StackRecord, StackStoreError>`;
  - `list(&self, project_id) -> Result<Vec<StackRecord>, StackStoreError>` (sorted by `stack_id`);
  - `StackRecord { stack_id, project_id, created_by_session_id, created_at_unix_ms: u64, stack }`;
  - `StackStoreError::{InvalidProjectId(String), InvalidStackId(String), NotFound { project_id, stack_id },
    Malformed { path, message }, Io { path, source }}`.
- `Changeset.stack_id: Option<String>` (`model.rs`); `Changeset.stack` still present until green retires it.
- `tddy_pr_stack::stack_migration::migrate_session_stacks(sessions_base: &Path, projects_dir: &Path) -> MigrationReport`,
  `MigrationReport { migrated_session_ids, sessions_without_project, failures: Vec<MigrationFailure> }`,
  `MigrationFailure { session_id, message }`.
- Stub bodies are `todo!()` under `// TODO(project-stack-store): implement`.
- Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — store, sweep and swapped callers are all its own
**Concurrent with:** n1, n3, n4, n5, n6
**Blocks:** n7 `stack-plan-tools` (serves the store by `stack_id`), n9 `stack-web-by-id` (lists and opens stacks by id)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] `ProjectStackStore` with a per-stack lock (in-process mutex keyed by path + atomic rename)
- [ ] `Changeset.stack_id`; `Changeset.stack` retired from the model
- [ ] Move every production reader/writer (~60 sites, 8 crates) to the store
- [ ] "Has a stack" checks (`parent_is_pr_stack_orchestrator`, `next_goal_for_state_with_changeset`) read `stack_id` + the store
- [ ] Startup sweep: write stack → write `stack_id` → drop `stack`; idempotent; logs sessions with no project
- [ ] `stack_plan_json` enrichment reads the store; web unchanged
- [ ] Coder overview row and docs via wrap

## Technical Changes

### State A (Current)

- `Changeset.stack: Option<Stack>` (`tddy-changeset/src/changeset/model.rs:90`); writers go through
  `update_stack_atomic(session_dir, f)` (`stack.rs:249`, read-modify-write, no lock); readers through
  `read_changeset(dir)?.stack` and `read_stack_with_resolved_branches` (`:297`).
- ~60 production sites: tddy-pr-stack `stack_ops/*`, `assess.rs:306`; tddy-workflow-recipes
  `pr_stack/{mod,hooks,bridge}.rs` (reseed `mod.rs:423`, "has a stack" resume rule `mod.rs:276`, plan hook
  `hooks.rs:281`), `orchestrate_pr_stack/{bridge,pr_actions,hooks}.rs`; tddy-tools
  `server.rs:1257,1275,1296,1305,1543,1628`; tddy-agent-launch `child_spawn_handler.rs:20`,
  `svc_pr_status_for_caller.rs:124` (the `link_stack_node_to_child_session` call); tddy-daemon-rpc
  `pr_stack/ports.rs:426,436,519,713`; tddy-session-worktree `session_chain.rs:118,156,190,198`;
  tddy-session-activity `session_list_enrichment.rs:78`.
  _Re-verified 2026-11-12 on d020d488c: every line held except `svc_pr_status_for_caller.rs` (120 → 124) and
  `session_chain.rs` (188 → 190)._
- Per-project dir already exists: `<data>/projects/<project_id>/notifications.jsonl` with a safe-id guard
  (`project_notification_log/storage.rs:34`). Sweep precedent `mount_host.rs:252`.

### State B (Target)

- `<projects dir>/<project_id>/stacks/<stack_id>.yaml` holds a `StackRecord`; `stack_id` is a UUIDv7.
- `ProjectStackStore::update(project_id, stack_id, f)` serializes writers per stack.
- A session dir resolves its projects dir by the sessions layout (`<sessions_base>/sessions/<id>` →
  `<sessions_base>/projects`), the assumption `refresh_internal_statuses` already makes; its project id
  from its metadata.
- Sweep on daemon start, per mapped user.

### Delta

- **tddy-changeset**: `project_stack_store.rs`; `update_stack_atomic`/`read_stack_with_resolved_branches`
  re-expressed over the store (by session dir → stack id) so most callers change only their call.
- **tddy-pr-stack**: `stack_ops::*` take `&StackHandle` (store + ids) instead of `&Path`.
- **tddy-workflow-recipes**, **tddy-tools**, **tddy-daemon-rpc**, **tddy-agent-launch**,
  **tddy-session-worktree**, **tddy-session-activity**: call-site swaps.
- **tddy-pr-stack** (sweep body, library) + **tddy-daemon** (`mount_host.rs` call).

## Implementation Milestones

- [ ] Store + lock + record (unit)
- [ ] `stack_id` + seam re-expressed; tddy-changeset suites green
- [ ] stack_ops + recipe swapped; recipe suites green
- [ ] Remaining crates swapped
- [ ] Sweep + daemon call
- [ ] Every pr-stack suite in the touched crates green

## Testing Plan

**Level**: unit for the store and lock, integration for the sweep (temp dirs with legacy changesets),
existing pr-stack acceptance suites as the behaviour-preservation net (they are rewritten to build
stacks through the store, not by writing `changeset.stack`).

## Acceptance Tests

### tddy-changeset — `packages/tddy-changeset/tests/project_stack_store_acceptance.rs`
- `a_created_stack_is_stored_under_its_project_and_read_back`
- `concurrent_updates_to_one_stack_both_land`
- `a_project_id_that_escapes_the_projects_dir_is_refused`
- `listing_a_project_returns_each_of_its_stacks`

### tddy-pr-stack — `packages/tddy-pr-stack/tests/stack_migration_sweep_acceptance.rs`
- `a_legacy_changeset_stack_is_moved_into_the_project_and_replaced_by_its_id`
- `a_second_sweep_changes_nothing`
- `a_sweep_interrupted_after_writing_the_stack_completes_on_rerun_without_a_duplicate`
- `a_session_without_a_project_is_left_alone_and_reported`

### tddy-workflow-recipes — `packages/tddy-workflow-recipes/tests/pr_stack_store_resume_acceptance.rs`
- `a_migrated_orchestrator_resumes_into_orchestrate`
- `a_planned_stack_is_written_to_the_project_store_and_the_changeset_holds_only_its_id`

### tddy-session-activity — `packages/tddy-session-activity/tests/stack_plan_json_from_store.rs`
- `the_session_list_reports_the_same_stack_plan_json_before_and_after_migration`

### Unit tests (red, 2026-11-12)

- `tddy-changeset/src/project_stack_store.rs` `mod tests`: project ids / stack ids that name no single
  path segment are refused (rstest cases); `NotFound` on read and on update (update creates nothing); a
  project without `stacks/` lists none; distinct ids per create; update returns and persists the record,
  changing only `stack`; an unparseable record is `Malformed`; an update leaves no temp file.
- `tddy-pr-stack/src/stack_migration.rs` `mod tests`: no `sessions/` dir → empty report; a stackless
  session and a session with no `changeset.yaml` are not touched or reported; several orchestrators
  migrate and report sorted; a changeset carrying both `stack` and a stored `stack_id` keeps the stored
  copy; an unparseable changeset is a `failures` entry.

Red notes for `/green`: `a_migrated_orchestrator_resumes_into_orchestrate` goes through
`start_goal_for_session_continue` → `next_goal_for_state_with_changeset(state, &Changeset)`, which has no
session dir, so the resume rule can only see `stack_id` (not the store) unless the trait grows a parameter;
`tddy-session-activity` gained a dev-dependency on `tddy-pr-stack` for the sweep.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Store in `tddy-changeset`**, beside `Stack` and today's `update_stack_atomic`: every current caller
  already depends on it, so no crate edge is added. `tddy-pr-stack` (the obvious home) is not a
  dependency of tddy-session-activity / tddy-session-worktree / tddy-tools today. `/green` may move it if
  that is wrong.
- **One node, not two**: a store nobody reads is a layer split, and a dual-read period was ruled out.
  Score 5 is a warning — the boundary is recorded here for review.
- **Write stack before id**: a crash between leaves a duplicate-safe state (the sweep checks
  `created_by_session_id` before creating).
- **Older daemons cannot read a migrated stack.** No downgrade path.

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n3 `feature/orchestrators/start-effort`.

## TODO

- [x] Record initial discovery (`2026-11-10-project-stack-store-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
