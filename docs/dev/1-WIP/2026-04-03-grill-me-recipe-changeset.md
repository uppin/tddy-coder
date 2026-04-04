# Changeset: Grill-me workflow recipe (2026-04-03)

**Status**: ✅ PR-wrap validation complete (2026-04-05); commit after excluding local-only paths (`artifacts/`, unintended `plans/` copies) — use `./dev` for fmt/clippy/test

## Plan mode context (Updated: 2026-04-05)

Feature: **`grill-me`** recipe with **two goals** — **`grill`** (clarification via **`InvokeResponse.questions`** / tddy-tools path) and **`create-plan`** (markdown brief **`grill-me-brief.md`** from Q&A + inputs). Graph **`grill` → `create-plan` → `end`**. No **`tddy-tools submit`** requirement for either goal (same class as **free-prompting**).

**Repo persistence:** **Create plan** output is also documented for **version control** in the working tree: **[AGENTS.md](../../../AGENTS.md)** (**Documentation Hierarchy** → **`plans/`**), or a path named in **`docs/ft/`** when specified; default **`plans/<SOME-PLAN-NAME>.md`**. Runtime file remains **`session_dir/artifacts/grill-me-brief.md`** until copied or mirrored.

## Affected packages

- `packages/tddy-workflow-recipes` — `grill_me` module, resolver, `approval_policy`, tests
- `packages/tddy-coder` — CLI help, `cli_recipe` tests
- `packages/tddy-web` — `ConnectionScreen` recipe options
- `docs/ft/coder/` — PRD, `workflow-recipes.md`

## Implementation progress

**Last synced with code**: 2026-04-04 (requirements: two goals **Grill** / **Create plan**)

- [x] PRD + feature doc updated (`PRD-2026-04-03-grill-me-recipe.md`, `workflow-recipes.md`, **AGENTS.md** **`plans/`** convention)
- [x] `GrillMeRecipe`: goals **`grill`** and **`create-plan`**; **`start_goal`** **`grill`**; state **`Grill`**; graph with **`EndTask`**
- [x] `grill_system_prompt` / `create_plan_system_prompt`; hooks branch on task id; **Create plan** user prompt composed from **`feature_input`**, **`output`**, **`answers`**
- [x] Acceptance tests (resolver, policy, `backend_invoke`, `cli_recipe`, prompt tests)
- [x] Wire resolver, exports, policy (unchanged CLI name **`grill-me`**)
- [x] `tddy-web` recipe label updated (`ConnectionScreen.tsx`)
- [x] Full `./dev ./verify` (2026-04-04; repo root `.verify-result.txt`)
- [x] PR wrap re-check: `./dev cargo fmt --all`, `./dev cargo clippy --workspace --all-targets -- -D warnings`, `./dev ./verify` (2026-04-05; `.verify-result.txt` — all crates `0 failed`)

**Session context (planning run)**: `019d54d1-2833-7342-8059-1bacb28e75f4` — `grill-me` graph completed; workflow state and brief path described in `.workflow/*.session.json` / session `artifacts/` (do not commit session output into the repo).

## Milestones

- [x] PRD drafted
- [x] Acceptance tests (resolver, policy, recipe metadata, prompt invariants)
- [x] `GrillMeRecipe` + hooks + prompt module
- [x] Wire resolver, exports, policy
- [x] Web + docs + verification (run `./test` / `./verify`)

## Validation (2026-04-03)

- `./dev cargo fmt --all`, `./dev cargo clippy -p tddy-workflow-recipes -p tddy-core -p tddy-coder -- -D warnings` — clean
- `./dev ./verify` — full `cargo test` workspace green (see repo root `.verify-result.txt`)
- Manual review: no production `println!` in new paths; `grill-me` uses `log::` for plain CLI output like `FreePromptingRecipe`

### Change validation (@validate-changes)

**Last run**: 2026-04-04  
**Status**: Passed (with scope warnings)  
**Risk level**: Medium (PR scope), Low (build/tests/security for sampled paths)

**Changeset sync**

- Items above match current tree: `grill_me` module present; resolver/policy/CLI/web wired; tests and `./dev ./verify` pass.

**Build validation**

| Package | Status | Notes |
|--------|--------|-------|
| `tddy-core` | Pass | `./dev cargo build -p tddy-core` |
| `tddy-workflow-recipes` | Pass | |
| `tddy-coder` | Pass | |
| `tddy-tui` | Pass | Large diff vs `master` also in tree — split PR if unrelated to grill-me |

**Analysis summary**

- **Tests**: Full workspace `cargo test` via `./dev ./verify` — exit 0; `.verify-result.txt` shows all crates completed with `0 failed`.
- **Security**: No secrets in reviewed policy/resolver/CLI paths; session JSON under `~/.tddy/sessions/...` is local dev metadata (socket path, etc.), not for commit.
- **Code quality**: Recipe name `grill-me` is duplicated across `approval_policy`, `recipe_resolve`, and clap `value_parser` lists — same follow-up as free-prompting (single source of truth).

**Risk assessment**

| Area | Level |
|------|--------|
| Build | Low |
| Changeset alignment | Low (this file tracks grill-me) |
| Test infrastructure | Low |
| Production code | Low–medium (duplicate recipe strings; logging patterns mirror free-prompting) |
| Security | Low |
| PR / merge scope | Medium — `git diff master` includes non–grill-me packages (`tddy-tui`, `tddy-web`, integration tests, etc.); consider splitting commits |

**Refactoring / follow-ups**

- [ ] Before merge: ensure `artifacts/` and stray session files are not committed; keep `.cursor/` local or list in `.gitignore` as intended.
- [ ] Optional: centralize supported recipe names for CLI + resolver + policy.

### PR wrap validation (2026-04-05)

- **Changes / risks**: Grill-me recipe, TUI layout/clarification, presenter `grill_ask_answers`, resolver/policy/tests, docs. Medium merge scope (`tddy-tui`, `tddy-e2e`, web) — split only if reviewers want smaller PRs.
- **Tests**: Simulated `/validate-tests` — integration tests assert behavior, no skipped tests in grill-me paths; workspace `./dev ./verify` green.
- **Prod-ready**: No new `println!` in TUI paths; presenter uses file + delete pattern; no FIXME left in touched grill-me sources from this pass.
- **Clean code**: One fix: `clippy::doc_lazy_continuation` in `packages/tddy-e2e/tests/grpc_terminal_rpc.rs` (indent doc continuation line).
- **Lint**: `cargo fmt` + clippy `-D warnings` + full `cargo test` via `./dev`.

### From persistence path tests

- [x] Implement `persisted_grill_me_brief_path` in `grill_me/repo_plan.rs` (`repo_root/plans/<stem>.md`, stem validation).
- [x] `cargo test -p tddy-workflow-recipes --test grill_me_repo_plan` — 8/8 pass.

## Acceptance tests outline (Updated: 2026-04-05)

- Resolver accepts `grill-me`; error strings list `grill-me`
- Policy lists `grill-me`; skip session-doc approval matches free-prompting class
- Recipe: `start_goal` **`grill`**, `initial_state` **`Grill`**, `goal_ids` includes **`grill`** and **`create-plan`**, `uses_primary_session_document`, artifact basename
- **Grill** prompt references structured questions; **Create plan** prompt names `grill-me-brief.md` and required sections
- **Documentation:** **AGENTS.md** + **`docs/ft/coder/workflow-recipes.md`** + PRD describe **`plans/<SOME-PLAN-NAME>.md`** (or feature-doc path) for persisted briefs

## Testing plan

- `cargo test -p tddy-workflow-recipes`, `cargo test -p tddy-coder --test cli_recipe`, `./test` or `./verify` from repo root
