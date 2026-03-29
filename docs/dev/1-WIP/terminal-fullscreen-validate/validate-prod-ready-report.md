# Production Readiness

Read-only review of the terminal fullscreen / terminate-confirm worktree, aligned with [evaluation-report.md](./evaluation-report.md) and focused on the listed `tddy-web` and `tddy-coder` surfaces.

## Checklist

| Area | Status | Notes |
|------|--------|--------|
| User-visible auth errors | Partial | `auth-flow-error` in standalone `ConnectionForm` (`index.tsx`); daemon `ConnectionScreen` relies on `useAuth` without the same test id |
| User-visible LiveKit / token errors | Good | `data-testid="livekit-error"` in `ConnectedTerminal` (both entry points) and in `GhosttyTerminalLiveKit` when `status === "error"` |
| RPC / session errors | Good | `data-testid="connection-error"` on `ConnectionScreen` for list/start/connect failures |
| Post-connect stream failures | Gap | RPC output errors logged only; UI may stay “connected” without a clear banner |
| Token refresh failures | Gap | `console.warn` only; no `setStatus("error")` or user message |
| Console logging in prod paths | Risk | Unconditional `[LiveKit]`, `[terminal→server]`, markers, and chrome `console.info`/`debug` |
| Rust logging | N/A here | `tddy-coder` daemon uses `log::` / `RUST_LOG`; not changed in reviewed UI slice beyond auth entry |
| Cypress / env | OK for CI | `LIVEKIT_TESTKIT_WS_URL`, `CYPRESS_BASE_URL`, fixed ports 8889/8890, dev LiveKit JWT secrets |
| Destructive confirmations | OK pattern | `window.confirm` for Terminate (via `remoteTerminateConfirm`) and session delete (`ConnectionScreen`) |
| Stub OAuth / auth surface | Risk | `build_auth_service_entry` enables stub mode when `--github-stub-codes` is non-empty even without `--github-stub` |
| fuser port cleanup | Risk | `fuser -k` on 8889/8890 can terminate unrelated listeners (evaluation report) |
| Ship artifacts | Block | Screenshots, tesseract data, stray logs called out in evaluation — exclude from release |

## Findings by area

### Error handling and user-visible errors

- **`index.tsx` — auth-flow-error:** When `useAuth` reports `authError`, it is rendered with `data-testid="auth-flow-error"` on the pre-login standalone form. Helpful for e2e and users.
- **`index.tsx` / `ConnectionScreen.tsx` — livekit-error:** Token generation failures set local `error` and render `data-testid="livekit-error"` with daemon-specific vs standalone copy (daemon mentions `tddy-daemon` / LiveKit; standalone mentions `tddy-coder` and API key flags).
- **`GhosttyTerminalLiveKit.tsx`:** Initial connect / setup failures set `errorMsg` and `status === "error"` → visible `livekit-error`. Server participant disconnect drives a dedicated overlay (`terminal-coder-unavailable`) with clear copy.
- **Gaps:** (1) Async RPC consumer on stream failure calls `console.error` but does not transition `status` or surface an in-UI error. (2) Token refresh in the refresh timer catches failures with `console.warn` only; long-lived sessions could degrade silently. (3) Auto-reconnect on `RoomEvent.Disconnected` swallows failures to `console.warn` without user feedback.

### Logging: `console` vs `log::`

- **Rust (`run.rs`):** `build_auth_service_entry` has no direct logging; auth wiring is structural only. Daemon processes spawned from Cypress use `RUST_LOG` (e.g. `info` / `debug` for demos).
- **TypeScript — intentional opt-in:** `debugLogging` gates the `log` closure in `GhosttyTerminalLiveKit` (verbose lifecycle/dataflow).
- **TypeScript — always on (prod concern):**
  - `GhosttyTerminalLiveKit`: `console.log("[LiveKit]", …)` on several room events regardless of `debugLogging`.
  - `enqueueTerminalInput`: unconditional `console.log("[terminal→server]", …, Array.from(encoded))` on every input path (keyboard, resize, Stop) — high volume and may leak keystroke patterns in devtools.
  - `ConnectionTerminalChrome`: `console.debug` / `console.info` on fullscreen sync, Terminate, and fullscreen button — not gated.
  - `browserFullscreen.ts`, `liveKitStatusPresentation.ts`, `remoteTerminateConfirm.ts`: `console.debug` / `console.info` helpers.
  - **`tddyMarker.ts`:** Every marker emits `console.debug`, `console.info`, and **`console.error(JSON.stringify(payload))`**. Call sites include fullscreen enter/exit, terminate confirm, and **`shouldShowVisibleLiveKitStatusStrip` on every invocation** (called from `GhosttyTerminalLiveKit` render), so markers can fire very often and flood stderr-style diagnostics in the browser console.

### Configuration / env: Cypress, ports, `GITHUB_*`, LiveKit

- **`cypress.config.ts`:** `LIVEKIT_TESTKIT_WS_URL` required for tasks that start LiveKit-backed binaries; `DEV_API_KEY` / `DEV_API_SECRET` match testkit-style local tokens (acceptable for automated tests, not production secrets). `baseUrl` defaults to Storybook (`6006`) unless `CYPRESS_BASE_URL` is set.
- **Fixed web ports:** `startTddyCoderForConnectFlow` uses **8889**, `startTddyCoderForAuthFlow` uses **8890**, with `fuser -k PORT/tcp` on POSIX before bind.
- **GitHub / stub:** Cypress spawns `tddy-coder` with `--github-stub` and `--github-stub-codes=test-code:testuser` (single argv for `:` safety). No production `GITHUB_*` env vars in these files; real OAuth path remains in `run.rs` when client id/secret are provided and stub mode is off.

### Security

- **`window.confirm`:** `remoteTerminateConfirm.ts` uses native `confirm`; if `window.confirm` is missing (non-browser), it **refuses** termination (safe default). Same UX pattern as `ConnectionScreen` session delete confirm.
- **Stub OAuth:** `build_auth_service_entry` treats **`stub_mode = github_stub || non-empty github_stub_codes`**, so a mis-typed or leftover `--github-stub-codes` on a production-like invocation could enable stub auth without an explicit `--github-stub` flag (evaluation report issue).
- **fuser:** Killing whatever holds 8889/8890 can affect unrelated services on a developer machine during Cypress runs (local CI hazard, not shipped to end users).

### Performance

- **Fullscreen listeners:** `ConnectionTerminalChrome` registers `fullscreenchange` + `webkitfullscreenchange` on `document` and syncs state; cleanup on unmount. Effect depends on `resolvedFullscreenTargetRef` (ref object identity is stable). Reasonable cost.
- **Re-renders / markers:** `shouldShowVisibleLiveKitStatusStrip` runs during **every** `GhosttyTerminalLiveKit` render and calls `emitTddyMarker` + `logDebug` each time — couples presentation logic to high-frequency diagnostic emission; any parent-driven re-render amplifies work.
- **`enqueueTerminalInput`:** Per-keystroke `console.log` with full byte arrays is a measurable overhead in hot paths and noisy for performance profiling.
- **Cypress stdio:** `tddy-coder` daemon for connect/auth uses `stdio: ["ignore", "ignore", "ignore"]` to avoid pipe backpressure — correct for reliability; `tddy-demo` / `echo_terminal` still pipe to log files or memory buffers for readiness detection.

## Risks

1. **Silent degradation:** Token refresh and RPC stream errors may leave the UI looking healthy while the session is broken or stale.
2. **Console noise and data exposure:** Unconditional LiveKit logs, per-input `[terminal→server]` logs, and marker `console.error` JSON in production builds increase noise and may aid shoulder-surfing in shared devtools sessions.
3. **Stub auth activation:** Non-empty `--github-stub-codes` alone enables stub `AuthService` — configuration foot-gun for operators.
4. **Local machine safety:** `fuser -k` on fixed ports during e2e.
5. **Fullscreen errors:** `requestFullscreenForConnectedTerminal` can throw; `handleFullscreenClick` uses `void requestFullscreen…` without `.catch`, risking unhandled promise rejection in strict monitoring environments.
6. **Ship hygiene:** Untracked artifacts listed in the evaluation report must not reach release artifacts or commits.

## Recommendations

1. **User-visible failure paths:** On RPC stream error (and optionally repeated reconnect failure), set terminal `status` to `"error"` or show a non-blocking banner so users know the stream ended.
2. **Token refresh:** After refresh failure, surface a message or force a controlled disconnect / reconnect prompt instead of only `console.warn`.
3. **Logging policy:** Gate `[LiveKit]` room-event logs and `[terminal→server]` behind `debugLogging` (or a single `import.meta.env.DEV` policy if the project agrees — without test-only branches in production behavior per workspace rules, prefer user-toggle or build-time strip).
4. **Markers:** Move `emitTddyMarker` off the hot render path for `shouldShowVisibleLiveKitStatusStrip`, or emit only on status transitions / user actions; avoid `console.error` for non-errors if markers are retained in production.
5. **Fullscreen:** Wrap `requestFullscreenForConnectedTerminal` in try/catch in the click handler (or `.catch`) and optionally show a short inline toast or status hint if the API rejects (user gesture, permissions).
6. **Rust auth CLI:** Require both explicit `--github-stub` and codes for stub mode in production-facing documentation; consider tightening `build_auth_service_entry` so non-empty codes alone do not enable stub (breaking change — coordinate with tests that rely on combined argv).
7. **Cypress:** Prefer port 0 / ephemeral allocation with discovery, or document that 8889/8890 must be free; avoid `fuser -k` where possible or scope to known PIDs.
8. **Release checklist:** Exclude screenshots, OCR training data, and `.cypress-red-output.txt` from shipping (per evaluation report).
