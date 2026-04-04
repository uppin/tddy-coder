# Terminal zoom — validate-tests report

Generated as part of the `validate-tests` subagent run for `feature/terminal-zoom-pitch`.

## Executive summary

| Metric | Value |
|--------|-------|
| **Overall** | **20 passed**, **0 failed** |
| Bun (`src/lib`) | 9 pass / 0 fail |
| Bun (`src/components/connection`) | 6 pass / 0 fail |
| Cypress component (`TerminalZoomAcceptance.cy.tsx`) | 5 pass / 0 fail |
| Skipped / pending | 0 |

All requested commands exited **0**.

## Commands run + exit codes

| # | Command | Exit code |
|---|-----------|-----------|
| 1 | `./dev bun test packages/tddy-web/src/lib` | **0** |
| 2 | `./dev bun test packages/tddy-web/src/components/connection` | **0** |
| 3 | `./dev bunx cypress run --component --project packages/tddy-web --spec packages/tddy-web/cypress/component/TerminalZoomAcceptance.cy.tsx` | **0** |

**Notes:**

- The nix dev shell printed a routine warning that the git worktree is dirty.
- One run printed `error (ignored): SQLite database ... eval-cache ... is busy` from nix; the test command still completed successfully (exit 0).
- Cypress reported dependency resolution / lockfile save on first `bunx` resolution; the run completed normally.

## Per-suite results

### 1. Bun — `packages/tddy-web/src/lib`

**9 tests, 0 failures** across 4 files:

| File | Tests (approx.) | Focus |
|------|-------------------|--------|
| `browserFullscreen.test.ts` | 1 | Unrelated to zoom |
| `remoteTerminateConfirm.test.ts` | 1 | Unrelated to zoom |
| **`terminalZoom.test.ts`** | **5** | **`clampTerminalFontSize`, `pitchInFontSize`, `pitchOutFontSize`, `canPitchIn`, `canPitchOut`** |
| `liveKitStatusPresentation.test.ts` | 2 | Unrelated to zoom |

### 2. Bun — `packages/tddy-web/src/components/connection`

**6 tests, 0 failures** across 2 files:

| File | Tests | Focus |
|------|-------|--------|
| `agentOptions.test.ts` | 3 | Agent select / RPC mapping |
| `connectionChromeStatus.test.ts` | 3 | `dataConnectionStatusValue` |

No connection tests are zoom-specific; they validate adjacent connection helpers only.

### 3. Cypress component — `TerminalZoomAcceptance.cy.tsx`

**5 tests, 0 failures** (~6–7s wall clock):

| Test | What it asserts |
|------|-----------------|
| `terminal_pitch_in_increases_font_...` | Pitch-in raises `data-terminal-font-size`; resize callback; cell count vs fixed viewport |
| `terminal_pitch_out_decreases_font_...` | Pitch-out lowers font; resize behavior |
| `terminal_zoom_reset_restores_baseline_font_...` | Reset returns to baseline font and cols/rows |
| `terminal_zoom_respects_configured_min_max_bounds` | Default bounds 8–32; buttons disabled at limits |
| `livekit_resize_osc_enqueued_when_dimensions_change_after_zoom` | `GhosttyTerminalLiveKit` path; resize OSC in logged output after zoom |

**Cypress counters:** Tests 5, Passing 5, Failing 0, Pending 0, Skipped 0.

## Coverage gaps / recommendations

### Well covered

- **Pure math / bounds** in `terminalZoom.ts` via `terminalZoom.test.ts`.
- **End-to-end zoom behavior** (toolbar → bridge → terminal → DOM + resize) via Cypress with `GhosttyTerminal` + `ConnectionTerminalChrome`, plus one LiveKit-flavored scenario.

### Gaps (recommended follow-ups)

1. **`terminalZoomBridge.ts` (no dedicated unit tests)**  
   - `dispatchTerminalZoomBridge` / `dispatchTerminalFontSizeSync` dispatch `CustomEvent`s on `window`. Could add small tests with a JSDOM-like `window` or a test harness that records dispatched events (action, baseline, opts; font size sync).

2. **`TerminalZoomToolbar.tsx` (no isolated test)**  
   - Toolbar disabled state tracks `TERMINAL_FONT_SIZE_SYNC_EVENT`. Acceptance tests exercise the full stack but do not assert toolbar-specific behavior in isolation (e.g. live state after sync without clicking).

3. **`GhosttyTerminal` `applyFontSizePx` (no unit / shallow test)**  
   - Logic (clamp → `term.options.fontSize` → `fit` → `setDisplayFontSize` → `dispatchTerminalFontSizeSync`) is only covered by Cypress integration. A focused test with a mocked `Terminal` / `FitAddon` would document invariants (clamping, sync dispatch) without a full browser terminal.

4. **Imperative `setTerminalFontSize` on the ref**  
   - Exposed on `GhosttyTerminalHandle` but not explicitly covered by automated tests visible in this pass.

5. **Keyboard shortcuts**  
   - Current `GhosttyTerminal` zoom path is **toolbar + `TERMINAL_ZOOM_BRIDGE_EVENT`**; there is no separate keyboard shortcut handler for zoom in the reviewed code. If product requirements add Ctrl/Cmd +/− zoom, those would need new tests (component or e2e).

6. **Non-default `minFontSize` / `maxFontSize` on `GhosttyTerminal`**  
   - Cypress bounds test uses defaults (8–32). Custom min/max on props + matching toolbar props are not explicitly validated.

7. **`ConnectionTerminalChrome.tsx`**  
   - Zoom UI is composed here; behavior is covered indirectly via Cypress. No dedicated unit test for chrome layout or wiring unless added later.

### Optional broader runs (not executed in this pass)

- Full `./dev bun test` for all `packages/tddy-web` tests.
- Full Cypress component suite (`bun run cypress:component` without `--spec`).

## Tests skipped or not run

- **Skipped / pending:** None in the Cypress run (0 skipped, 0 pending).
- **Not run (by design of this validation):** Full-repo web test sweep; Cypress e2e; Storybook-only checks.
