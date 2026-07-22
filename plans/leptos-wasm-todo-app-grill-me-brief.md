# Leptos WASM browser TODO app (greenfield demo)

## Problem

Build a **greenfield demo / learning** TODO web application that exercises the repo’s plan-tdd workflow end to end. Users need a **browser** client to **add**, **list**, **toggle complete**, and **delete** tasks as a **single user**, with tasks **surviving refresh and restart** via **browser localStorage**. The app should ship as a **static build** (served `dist/`-style assets), live under a **top-level app directory** (not inside `tddy-web`), and use **Leptos → WASM** on the client with **Rust unit/integration tests** plus **Playwright** e2e coverage.

This is intentionally isolated from `tddy-daemon`, session RPC, and workflow `TODO.md` artifacts—it is a standalone product demo, not a dashboard feature.

## Q&A

| Topic | Question (summary) | User decision |
|--------|-------------------|---------------|
| Where it lives | Purpose in tddy-coder context? | **Greenfield demo / learning project** (full plan-tdd workflow; not required to merge into core packages) |
| Client surface | What users interact with? | **Web app (browser)** |
| MVP scope | First shippable capabilities? | **Minimal list**: add, list, toggle complete, delete; single user; persistence across restart |
| Stack | Implementation stack? | **Rust full-stack** → clarified as **Rust → WASM in the browser** |
| Persistence | Where tasks survive restart? | **Browser localStorage** |
| Run / demo | How to run and demo? | **Hosted static build** (`dist/`; serve or open built assets) |
| Rust + storage | How Rust and localStorage fit? | **Leptos compiled to WASM**; task data only in the browser |
| Repo placement | Where code lives? | **Top-level app directory** (e.g. `apps/todo/`) |
| Quality bar | Testing depth? | **Rust tests + browser/e2e** |
| WASM UI | Leptos vs alternatives? | **Leptos** |
| E2E runner | Playwright vs Cypress? | **Playwright** |

## Analysis

### Architecture

- **Client-only data plane**: No task API or server-side DB. A small **static file server** (or `trunk serve` / `cargo leptos serve` in dev) is only for hosting WASM/JS/CSS; persistence is **`localStorage`** with a versioned JSON schema.
- **Leptos + WASM**: Matches “Rust full-stack” while honoring localStorage and static delivery. Core task logic should be testable in **native Rust unit tests** (model + serialization + storage adapter traits); WASM bindings call the same logic where feasible.
- **Location `apps/todo/`**: First `apps/` product in this monorepo (existing `apps/` references are build examples under `tddy-build-typescript`). Add as a **Cargo workspace member** (path `apps/todo` or nested crate layout) without coupling to `tddy-web`’s React/Vite stack.

### Trade-offs

| Choice | Benefit | Cost |
|--------|---------|------|
| localStorage | Zero backend ops; fits static hosting | No sync, no multi-device; storage limits; manual export/import out of scope for MVP |
| Leptos WASM | Single language; aligns with Rust workspace | Toolchain setup (wasm32, trunk/leptos build); CI must install wasm target and Playwright browsers |
| Playwright (not Cypress) | User preference; `tddy-web` already documents `*.pw.ts` patterns | New package config; not wired into root `bun run test` by default |
| Isolated `apps/todo` | Minimal blast radius on daemon/web | Root `./test` will **not** run app tests until explicitly integrated—document local verify commands |

### Risks and mitigations

- **Workspace bloat / compile time**: Keep one crate (or lib + bin split) with focused deps; avoid pulling daemon crates.
- **localStorage in Playwright**: Use `storageState` or per-test `page.evaluate` seeding; prefer **`data-testid`** and roles for stable selectors (see fluent-tests Playwright reference).
- **Hydration / SSR**: MVP should be **CSR-only** (no SSR) to simplify static hosting.
- **Nix dev shell**: Ensure `wasm32-unknown-unknown` and any Leptos/Trunk CLI are documented in app README; use `./dev` for consistency.

### Dependencies

- Leptos ecosystem (version pinned in app `Cargo.toml`; use current stable compatible set).
- Optional: **Trunk** or **cargo-leptos** for build/serve (implementer picks one and documents it).
- **Node/Bun** for Playwright only (can use `package.json` in `apps/todo/` without adding to root workspaces initially).

### Open questions (non-blocking; implementer decides with brief rationale in PRD/TODO)

- Exact app directory name (`apps/todo` vs `apps/leptos-todo`).
- Whether to hook `apps/todo` tests into root CI in v1 or keep `apps/todo/README.md` verify script only.
- Minimal styling (plain CSS vs utility framework)—default to accessible, readable defaults; no design system requirement.

## Preliminary implementation plan

### Phase 1 — Scaffold and build

- Create `apps/todo/` (or agreed name) with Leptos WASM app skeleton, `index.html`, and build output to `dist/`.
- Register crate in root `Cargo.toml` `members` (if using workspace crate).
- Document **dev** (`serve` with hot reload) and **release** (`build` → static `dist/`) in app README.
- Add `data-testid` hooks on: new-task input, add button, task list, per-task checkbox, delete control, empty state.

### Phase 2 — Domain and persistence (TDD)

- Define `Todo` / `TodoList` model (id, title, completed, created_at optional).
- Implement `localStorage` repository: load on startup, save on every mutation; handle corrupt/missing data with clear empty state (no silent fallback to fake data—surface empty list or explicit error UI per project rules).
- **Rust tests**: serialization round-trip, CRUD operations on in-memory adapter, localStorage adapter via test double or `wasm-bindgen-test` only where needed; prefer native tests for logic.

### Phase 3 — UI

- Leptos components: header, input + add, filter optional **out of scope** for MVP, list with checkbox + label + delete.
- Keyboard: Enter to add; focus management after add.
- Empty state when no tasks.

### Phase 4 — Playwright e2e

- Add `apps/todo/playwright.config.ts` (mirror patterns from `packages/tddy-web/playwright.config.ts`: `*.pw.ts` suffix, chromium project).
- Fluent-tests style: page object (`TodoPage`), Given/When/Then specs for: add task, complete task, delete task, persistence after reload.
- Script: `bunx playwright test` (or npm) from app dir; document browser install (`playwright install chromium`).

### Phase 5 — Demo and handoff

- Optional `apps/todo/demo.sh` per AGENTS.md: launch static server in separate terminal for demo (if product wants demo script).
- Summarize verify: `./dev cargo test -p <crate>` + Playwright from app directory.

### Out of scope (v1)

- Auth, multi-user, sync, backend API, due dates/priorities/tags, import/export, PWA/offline service worker, integration into `tddy-web` or daemon RPC, root `./test` wiring unless explicitly added.

### Success criteria

- Static `dist/` loads in browser; tasks persist across reload.
- All new Rust tests pass; Playwright happy-path suite passes against served `dist/` or dev server URL configured in Playwright.
- Code lives under `apps/` with no changes required to `tddy-daemon` or `tddy-web` for core behavior.
