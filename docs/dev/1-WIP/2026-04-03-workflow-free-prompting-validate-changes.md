# Workflow free-prompting + approval policy — @validate-changes

**Status**: 🚧 In Progress (working tree; not fully merged to `master`)

## Implementation Progress

**Last synced with code**: 2026-04-03 (via @validate-changes)

**Core features** (from `git diff master` scoped to workflow packages):

- [x] `free-prompting` recipe (`FreePromptingRecipe`, `free_prompting/` graph, hooks) — ✅ Complete in tree (`packages/tddy-workflow-recipes/`)
- [x] `approval_policy` helpers — ✅ Complete in tree
- [x] `recipe_resolve` + `unknown_workflow_recipe_error` — ✅ Complete in tree
- [x] Core `WorkflowRecipe` / runner / task adjustments — ✅ Complete in tree (`packages/tddy-core/`)
- [x] Presenter `workflow_runner` bootstrap (e.g. recipe in changeset) — ✅ Complete in tree
- [x] CLI / backend recipe selection (`tddy-coder` `run.rs`, `backend/mod.rs`) — ✅ Complete in tree
- [x] Integration tests (presenter, workflow-recipes, e2e, integration-tests) — ✅ Present in tree
- [~] Product docs — ⚠️ Partial: committed docs commit on branch; additional edits in working tree under `docs/ft/coder/`

**Additional discovery**:

- [~] **Branch contents vs `master`**: Working tree diff includes **non-workflow packages** (`tddy-tui`, `tddy-web`, `tddy-livekit`, etc.). For a focused PR, split or rebase so only workflow-related commits/files ship.

**Testing**:

- [ ] Full `./test` or `./verify` not run in this validation pass (build only).

### Change Validation (@validate-changes)

**Last run**: 2026-04-03  
**Status**: ⚠️ Warnings  
**Risk level**: 🟡 Medium

**Context documents**:

- Cross-ref: `docs/dev/changesets.md` (2026-03-29 free prompting bullet)
- Validation notes: `docs/dev/1-WIP/workflow-free-prompting-validate/` (evaluation / prod-ready / test reports)
- No separate PRD under `docs/ft/*/1-WIP/` for this slice (PRD lived in session artifacts during planning)

**Changeset sync**:

- No prior file with `🚧 In Progress` matched in `docs/dev/1-WIP/*.md`; this file establishes tracking.
- `git diff master...HEAD` (committed on branch): **documentation only** (4 files).
- **Uncommitted / full tree vs `master`**: implements free-prompting + policy + tests (see git status).

**Build validation** (`./dev cargo build -p tddy-core -p tddy-workflow-recipes -p tddy-coder -p tddy-e2e -p tddy-integration-tests`):

| Package               | Status   | Notes                          |
|-----------------------|----------|--------------------------------|
| tddy-core             | ✅ Pass |                                |
| tddy-workflow-recipes | ✅ Pass |                                |
| tddy-coder            | ✅ Pass |                                |
| tddy-e2e              | ✅ Pass |                                |
| tddy-integration-tests| ✅ Pass | (incremental after core graph) |

**Analysis summary**:

- Files touched (workflow scope): `tddy-workflow-recipes` (new recipe, policy, tests), `tddy-core` (recipe trait, runner, task, presenter, stub, backend), `tddy-coder` (CLI + presenter tests), `tddy-e2e`, `tddy-integration-tests`.
- Critical issues: **mixed-feature diff vs `master`** (see above).
- Warnings: `FreePromptingRecipe::plain_goal_cli_output` logs full agent output at `info` (noise / sensitivity); `approval_policy` vs `WorkflowRecipe::uses_primary_session_document` can drift (see existing validate-prod-ready report); duplicate recipe name lists across resolver / clap / TUI.
- Security: no secrets observed in sampled paths.

**Risk assessment**:

| Area              | Level  |
|-------------------|--------|
| Build             | 🟢 Low |
| Changeset alignment | 🟡 Medium (docs vs full tree) |
| Test infrastructure | 🟢 Low (stubs used as designed) |
| Production code   | 🟡 Medium (logging, enum drift) |
| Security          | 🟢 Low |
| Code quality      | 🟡 Medium (duplication, long presenter test additions) |

### Refactoring / follow-ups (from validation)

- [ ] Run full `./verify` before merge; attach `.verify-result.txt` evidence.
- [ ] Trim or downgrade verbosity of `[free-prompting] output` in `plain_goal_cli_output` for production logs.
- [ ] Single source of truth for `tdd` / `bugfix` / `free-prompting` strings (resolver + clap + TUI).
- [ ] Do not commit `.tddy-workflow-recipes-red-test-output.txt` or stray `Queue` artifact.
- [ ] Split PR if `tddy-tui` / `tddy-web` / LiveKit changes are unrelated to free-prompting.
