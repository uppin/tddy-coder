# Clean Code Analysis

Scope: terminal fullscreen, connection chrome, terminate confirm, LiveKit status presentation, `build_auth_service_entry`, and Cypress e2e spawn tasks. Informed by [evaluation-report.md](./evaluation-report.md) (PRD alignment, medium risk, harness issues).

## Strengths

- **Helper vs UI split:** `browserFullscreen.ts`, `remoteTerminateConfirm.ts`, and `liveKitStatusPresentation.ts` keep DOM/browser details and policy out of JSX. Public APIs are narrow and named for behavior (`requestFullscreenForConnectedTerminal`, `exitDocumentFullscreen`, `confirmRemoteSessionTermination`, `shouldShowVisibleLiveKitStatusStrip`).
- **Vendor fullscreen isolation:** Prefixed fullscreen enter/exit and `isTargetInActiveFullscreen` live in one module with small typed wrappers—easier to test and extend than scattering prefixes in components.
- **Terminate flow:** Copy stays in `ConnectionTerminalChrome`; confirmation is delegated to `confirmRemoteSessionTermination` (marker + safe refusal when `confirm` is missing)—good single place to mock or swap UX later.
- **Chrome component cohesion:** `ConnectionTerminalChrome` (~295 lines) owns dot, menu, fullscreen control, and Stop in one tree; props are explicit (`overlayStatus`, optional `onTerminate`, `fullscreenTargetRef`). Internal fullscreen fallback wrapper is a clear, documented branch.
- **Parent wiring:** `GhosttyTerminalLiveKit` passes `fullscreenTargetRef` into chrome so fullscreen can target the flex terminal subtree or an external container—clean dependency direction (parent owns layout node).
- **Observability pattern:** `emitTddyMarker` usage across the three `lib` modules is consistent for cross-cutting traces.
- **Rust `build_auth_service_entry`:** One function, two main branches (stub vs real GitHub), with a short comment explaining why non-empty `--github-stub-codes` enables stub mode—readable policy in one place.
- **Cypress lifecycle:** `stopSpawnedProcessTree`, dedicated stop helpers, and `after:run` cleanup keep process management localized; `getTddyCoderPath` comment documents why debug binary is preferred.

## Issues

- **Naming consistency:** “Chrome” (`ConnectionTerminalChrome`, `LiveKitChromeStatus`) vs “overlay” (`connectionOverlay`, `overlayStatus`) describes the same UX layer with two vocabularies—fine for experts but slightly noisy for readers jumping between files.
- **Component size / SRP:** `GhosttyTerminalLiveKit.tsx` is very large (~590+ lines). The main `useEffect` bundles token refresh, room connect, participant wait, transport, streaming loop, buffer flush, and status updates—hard to review, test in isolation, or reuse.
- **Logging consistency:** `debugLogging` gates a `log` helper, but many `console.log`/`console.warn` calls for LiveKit events are unconditional—noisy in production and uneven with the chrome’s `console.debug`/`console.info` usage.
- **`shouldShowVisibleLiveKitStatusStrip`:** The implementation only branches on `connectionOverlayEnabled`; `status` is forwarded to markers/debug but does not change the boolean result. That is either dead parameter surface or future-proofing that should be documented in the function body (not only the file-level JSDoc) to avoid “lying” signatures.
- **`liveKitStatusPresentation` testability:** The function is trivially unit-tested, but the *product* rule (“strip hidden when overlay on, errors still surfaced via `livekit-error`”) spans `GhosttyTerminalLiveKit` JSX—integration/e2e carries more of the truth than the helper alone.
- **`build_auth_service_entry`:** Stub and real paths both construct `AuthServiceImpl` + `ServiceEntry` with duplicated structure; acceptable at current size but will grow together if more providers appear.
- **Cypress config size:** `setupNodeEvents` embeds long task implementations; the file mixes config, path helpers, JWT boilerplate, and process orchestration—high cognitive load (see Duplication).

## Duplication

- **Fullscreen:** No meaningful duplication between chrome and `browserFullscreen.ts`; chrome calls the lib only. `GhosttyTerminalLiveKit` duplicates the *pattern* of “optional external ref vs internal `useRef`” with `ConnectionTerminalChrome`—mirrored intentionally, not copy-pasted logic.
- **Cypress `startTddyCoderForConnectFlow` vs `startTddyCoderForAuthFlow`:** Near-duplicate: same env checks, binary/bundle validation, `fuser` prelude, spawn args (mostly identical), readiness polling, timeout handling—differs mainly by port (8889 vs 8890), process handle variable, and task names. High maintenance cost when flags or args change.
- **JWT grant setup:** Repeated across `startTerminalServer`, `startEchoTerminal`, and the tddy-coder tasks with the same `DEV_API_KEY` / room grants—could be a small shared helper inside the config file.

## Suggested refactors (prioritized)

1. **Extract a shared factory for Cypress “start tddy-coder web daemon” tasks** — Parameterize port, process slot, and optional extra CLI args; keep task names as thin wrappers. Reduces drift and aligns with evaluation-report concerns about port hygiene (single place to document `fuser` risk).

2. **Decompose `GhosttyTerminalLiveKit` room/stream effect** — Move the async `run()` body into a custom hook or module (`useLiveKitTerminalSession` / `liveKitTerminalSession.ts`) that returns status, error, enqueue function, and refs/callbacks. Keeps the component as composition + render only; improves testability of connection logic (mock room/transport in tests).

3. **Unify logging policy** — Either route LiveKit lifecycle logs through the existing `debugLogging` gate or a shared `liveKitLog(level, …)` helper so production consoles are predictable; reserve unconditional logs for true errors.

4. **Clarify or slim `shouldShowVisibleLiveKitStatusStrip`** — If `status` will never affect visibility while overlay is on, remove it from the args object and markers, or add a one-line comment in the function that `status` is retained for analytics/markers only. Avoid APIs that suggest a decision tree that does not exist.

5. **Optional: `useFullscreenState(targetRef)` hook** — Encapsulate `fullscreenchange` / `webkitfullscreenchange` subscription and `isTargetInActiveFullscreen` syncing in `browserFullscreen.ts` (or adjacent). `ConnectionTerminalChrome` shrinks; behavior stays testable by driving `document` events in tests.

6. **Vocabulary pass (low priority)** — Pick “overlay” or “chrome” in prop/type names in a follow-up PR, or document the distinction (chrome = controls, overlay = connection feature flag) in one module-level comment to reduce onboarding friction.
