# Grill-me test: minimal TODO app

## Problem

The user is exercising the **grill-me** workflow end-to-end: TUI elicitation (`tddy-tools ask`), structured PRD handoff to the Claude CLI (`tddy-tools submit --goal plan`), and this **Create plan** brief for implementation handoff.

The substantive product goal embedded in the test is a **minimal TODO application**—enough scope to validate that a spawned implementation conversation can execute against a real, small PRD without pulling in unrelated platform work.

## Q&A

| Question | User decision |
|----------|----------------|
| How are you? | **Great** |

No other clarification questions were asked in the Grill phase.

## Analysis

- **Intent:** Primarily a **workflow smoke test**; the TODO app is a deliberately small stand-in feature with clear acceptance criteria.
- **Already submitted PRD (plan goal):** Minimal TODO app with add/list/complete/delete, local persistence (e.g. browser `localStorage`), simple single-page UI. **Out of scope:** authentication, multi-device sync, native mobile apps.
- **Risks:** Low for product; main risk is conflating “test the pipeline” with “ship production TODO product in tddy-web”—implementation should stay scoped to a demo or isolated sample unless the user later expands scope.
- **Dependencies:** None on tddy-core/daemon changes for a standalone web demo; if placed under `packages/tddy-web`, follow existing Bun/Vite/Cypress conventions and `fluent-tests` for any new tests.
- **Open questions:** Stack choice (standalone Vite page vs. Storybook story vs. new route in tddy-web) is left to the implementer unless the orchestrator specifies otherwise; default to the smallest path that satisfies the PRD.

## Preliminary implementation plan

### Phase 1 — Scaffold

- Choose location (e.g. small standalone demo under repo or a minimal addition to `packages/tddy-web`).
- Define task model: `id`, `title`, `completed`, optional `createdAt`.

### Phase 2 — Core behavior

- **Add** task (non-empty title).
- **List** tasks with visual distinction for completed items.
- **Toggle complete** and **delete** per task.
- Load/save full task list to **local persistence** on change.

### Phase 3 — Polish and verify

- Basic layout and accessible controls (labels, keyboard-friendly inputs).
- Manual smoke test in browser.
- If tests are added, use **fluent-tests** style per repo guidelines.

### Phase 4 — Done criteria

- User can manage a task list end-to-end in the browser without a backend.
- Changes are documented in commit message; no secrets or env files committed.
