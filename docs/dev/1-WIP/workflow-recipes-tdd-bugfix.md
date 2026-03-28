# Workflow recipes: TDD vs Bugfix (developer reference)

This document satisfies PRD acceptance criteria for documenting how **TDD** and **Bugfix** recipes map to the same product philosophy as the repo’s Cursor-oriented commands.

## TDD (`tdd`)

- **Default** when `--recipe` is omitted or `changeset.yaml` has no `recipe` field (backward compatible).
- **Start goal:** `plan` — greenfield planning, PRD/TODO-style artifacts, full graph (plan → acceptance-tests → red → green → …).
- **Spirit:** Aligns with a typical feature-development workflow (plan first, then tests and implementation).

## Bugfix (`bugfix`)

- **Start goal:** `reproduce` — confirm or create a failing test / deterministic reproduction before changing production code.
- **Artifacts:** Primary session document is a **fix plan** (e.g. `fix-plan.md` under the session artifact layout), not only PRD semantics.
- **Spirit:** Maps to the ideas behind `.cursor/commands/reproduce.md` (reproduction discipline) and `.cursor/commands/fix-tests.md` (focused diagnosis and fix, small verification loops).
- **Gate:** After reproduce, the user **previews** the session document and **approves or rejects** before **green** / fix implementation runs (same approval machinery as plan review where applicable).

## Selecting a recipe

| Surface | How |
|--------|-----|
| **tddy-coder** | `--recipe tdd` or `--recipe bugfix` (optional YAML `recipe:`; CLI wins). |
| **Web** | “Workflow recipe” control on **Start New Session**; value is sent on `StartSession` / `StartSessionRequest` as `recipe`. |
| **Daemon** | Normalizes empty recipe to `tdd`; persists `recipe` on the session `changeset.yaml`. |

## Tests

- **Rust:** `./test` from the repo root is the primary gate (builds required binaries including `tddy-acp-stub`, then runs `cargo test` with `--test-threads=1`).
- **Web:** Cypress component/e2e for `tddy-web` are **not** included in `./test`; run from the repo via `bun run cypress:component` / `cypress:e2e` under `packages/tddy-web` (or root scripts that filter `tddy-web`). Ensure workspace install so `tddy-livekit-web` resolves (Vite aliases `tddy-livekit-web` to package source for dev/Cypress).
