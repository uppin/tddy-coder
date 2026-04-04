# Validation report: terminal reconnect overlay tests

**Date:** 2026-04-04  
**Scope:** Automated validation for the terminal reconnect overlay work in `packages/tddy-web` (routing helpers, `terminalPresentation` logic, ConnectionScreen acceptance tests), plus quick Rust checks where feasible.

## Commands run + exit codes

| Command | Exit |
|--------|------|
| `./dev bun test --cwd packages/tddy-web src/routing/appRoutes.test.ts src/components/connection/terminalPresentation.test.ts src/components/ConnectionScreen.test.tsx` | **0** |
| `./dev bun test --cwd packages/tddy-web src/routing src/components/connection` | **0** |
| `./dev cargo check -q` | **0** |
| `./dev cargo test -q -p tddy-tools` | **0** (completed in ~142s; slow due to compile + multiple crates) |

## Results table

| Suite | Passed | Failed | Notes |
|-------|--------|--------|--------|
| Bun — targeted (3 files) | 15 | 0 | `appRoutes`, `terminalPresentation`, `ConnectionScreen.test.tsx`; 31 `expect()` calls; ~111ms |
| Bun — broader (`src/routing`, `src/components/connection`) | 19 | 0 | Adds `agentOptions.test.ts`, `connectionChromeStatus.test.ts`; ~39ms |
| `cargo check -q` (workspace) | — | — | Succeeded (~6.4s); no errors |
| `cargo test -q -p tddy-tools` | 61 | 0 | All reported crates: ok; total runtime ~142s |

**Failures / flakes:** None in this run. **Flaky patterns:** Not assessed (single execution per command).

## Missing coverage / gaps (vs PRD and product intent)

1. **`ConnectionScreen.tsx` not wired to `terminalPresentation` helpers**  
   The production component does not import or reference `terminalPresentation` (`attachKindForSessionControl`, `nextPresentationFromAttach`, etc.). Logic is covered in pure unit tests and in `ConnectionScreen.test.tsx`, but that file only imports the helpers and asserts behavior — it does **not** mount `ConnectionScreen` or assert that the real component calls the helpers on `resumeSession` vs `startSession` / `connectSession`.

2. **No Cypress (or other browser) tests specifically for the reconnect overlay presentation path**  
   Existing Cypress coverage includes `ConnectionScreen.cy.tsx`, Ghostty/LiveKit chrome, and e2e scenarios like `ghostty-concurrent-sessions.cy.ts` (reconnect blank-terminal bug), but there is no dedicated component/e2e test that asserts “resume → overlay, no route push” vs “new connect → full + push” in the live UI.

3. **PRD alignment (`docs/ft/web/web-terminal.md`)**  
   The PRD describes routing and connect/resume behavior (e.g. push to `/terminal/{id}` on certain flows). Current tests validate **helpers** and **canonical path rules**; end-to-end verification that the SPA applies overlay rules after navigation events is still thin.

4. **Rust scope**  
   `tddy-tools` tests passed; they do not exercise the web overlay. No regression signal for React from that crate.

## Recommendations

1. **Integrate** `terminalPresentation` into `ConnectionScreen.tsx` (or a single facade used only there) so production behavior cannot drift from the tested helpers without a compile-time or explicit call-site change.

2. **Add Cypress component or e2e tests** that drive Resume vs Connect/Start and assert URL and presentation (e.g. overlay vs full terminal region), reusing stable `data-testid`s where possible.

3. **Optional:** A shallow React test that mounts `ConnectionScreen` with stubbed RPC and asserts navigation/presentation side effects for `resumeSession` — only if mounting cost and flakiness are acceptable; otherwise prefer Cypress for DOM + router integration.

4. **CI:** Keep the targeted `bun test` paths in the PR checklist until Cypress coverage exists for the overlay path.
