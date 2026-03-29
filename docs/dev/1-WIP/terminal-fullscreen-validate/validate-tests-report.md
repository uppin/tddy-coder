# Validate Tests Report

## Commands Run

1. `./dev cargo test -p tddy-coder -- --list` — enumerate tests (completed after initial compile; exit 0).
2. `./dev cargo test -p tddy-coder` — full package test suite.
3. `./dev bash -c 'cd packages/tddy-web && bun test src/lib/browserFullscreen.test.ts src/lib/liveKitStatusPresentation.test.ts src/lib/remoteTerminateConfirm.test.ts && bun run test:unit'`.
4. `./dev bash -c 'cd packages/tddy-web && bunx cypress run --component --spec cypress/component/ConnectionTerminalChrome.cy.tsx,cypress/component/GhosttyTerminalLiveKit.cy.tsx,cypress/component/ConnectionScreen.cy.tsx'`.

**Not run (this pass):** Full Cypress e2e (`bun run cypress:e2e`), including `app-connect-flow` and other LiveKit-heavy specs; broader component suite beyond the three files above; `cargo test` for workspace packages other than `tddy-coder`.

## Results

| Suite | Passed | Failed | Skipped | Notes |
|--------|--------|--------|---------|--------|
| `cargo test -p tddy-coder` | 65 | 0 | 0 | 20 lib + 9 `cli_args` + 15 `cli_integration` + 2 `cli_recipe` + 1 `daemon_toolcall_poll_regression` + 1 `flow_runner` + 12 `presenter_integration` + 1 `sigint_session_output` + 4 `web_bundle_acceptance`; 0 doc tests |
| Bun focused (`browserFullscreen`, `liveKitStatusPresentation`, `remoteTerminateConfirm`) | 4 | 0 | — | Single run across 3 files |
| `bun run test:unit` (tddy-web) | 7 | 0 | — | Includes overlapping lib tests + `connectionChromeStatus.test.ts` |
| Cypress component (3 specs) | 35 | 1 | 0 | Overall run exit code 1 |

**Aggregate (executed):** 65 Rust passes; Bun: 4 in the focused trio, then `test:unit` reported 7 passes (includes the same lib tests plus `connectionChromeStatus`); Cypress **35** of **36** cases passed. **1** Cypress failure overall.

## Failures

| Location | Test / symptom |
|----------|----------------|
| `packages/tddy-web/cypress/component/ConnectionScreen.cy.tsx:439` | **ConnectionScreen terminal chrome — status dot menu › cancelling Terminate confirmation does not call SignalSession** — timed out after 10s: expected `[data-testid='connection-menu-disconnect']` to become visible after clicking the status dot. (Failure line is the assertion waiting for the disconnect menu item; root cause is likely menu not opening or timing/stub ordering in this scenario.) |

No Rust or Bun failures in the commands above.

## Coverage Gaps

Relative to [evaluation-report.md](./evaluation-report.md) (fullscreen, terminate confirm, LiveKit status visibility, app-connect harness):

| Area | Automated coverage this run | Gap |
|------|----------------------------|-----|
| **Fullscreen** | `browserFullscreen.test.ts`; CT: `ConnectionTerminalChrome` (control placement); `GhosttyTerminalLiveKit` (requestFullscreen stub enter path) | No CT/e2e asserting vendor-prefixed fullscreen or exit fullscreen in `ConnectionScreen` chrome path |
| **Terminate confirm** | `remoteTerminateConfirm.test.ts`; `GhosttyTerminalLiveKit` CT (confirm + cancel paths **pass**); `ConnectionScreen` CT cancel path | **ConnectionScreen** integration test for cancel **failed** — end-to-end confidence on that screen is currently broken in CI terms |
| **LiveKit status strip visibility** | `liveKitStatusPresentation.test.ts`; `GhosttyTerminalLiveKit` CT (“hides visible livekit status text…”) | `ConnectionScreen` flow asserts `livekit-status` not visible when overlay on (passes in other tests); no dedicated e2e here |
| **`run.rs` / stub auth (`--github-stub-codes`)** | No targeted test in `tddy-coder` asserts non-empty `github_stub_codes` enables stub auth | Evaluation calls out this Rust behavior; **no direct unit/integration test** was observed for that flag parsing path |
| **App-connect e2e** | Not executed | Evaluation mentions Cypress harness fixes for app-connect; **full e2e suite not run** (may need daemon/LiveKit/storybook serve as per project scripts) |

## Recommendations

1. **Fix or stabilize** `ConnectionScreen.cy.tsx` “cancelling Terminate confirmation…” — align with the passing pattern in the preceding test (e.g. wait for LiveKit/chrome readiness before opening the dot menu, or ensure `confirm` stub is installed before mount if order matters).
2. **Add a small Rust test** (or extend existing CLI/config tests) that documents and guards “non-empty `--github-stub-codes` implies stub auth mode” if that contract must stay stable.
3. **Run `bun run cypress:e2e`** (or at least `app-connect-flow.cy.ts`) in CI or before merge when validating the app-connect harness changes.
4. Keep **GhosttyTerminalLiveKit** CT cases as regression guards for fullscreen + terminate confirm; once ConnectionScreen cancel passes, coverage for terminate-cancel is consistent across chrome variants.
