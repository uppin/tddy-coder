# Code quality analysis: terminal reconnect overlay (web)

Scope: `terminalPresentation.ts`, `terminalPresentation.test.ts`, `appRoutes.ts`, `ConnectionScreen.test.tsx`.

## `terminalPresentation.ts`

### Strengths

- **Clear domain model**: Exported union types (`TerminalPresentation`, `TerminalAttachKind`, `SessionControlAction`, `TransitionCounters`) are small and readable; names match product language (overlay, mini, full, reconnect).
- **Single-responsibility functions**: Each exported function maps one transition or decision (`attachKindForSessionControl`, `nextPresentationFromAttach`, `reconcileReconnectOverlayInstances`, overlay click, dedicated back, placement). Low cyclomatic complexity; easy to reason about.
- **Documentation**: File-level and function-level comments tie behavior to the PRD and explain *why* (e.g. show-at-most-once overlay, no second connect on overlay expand).
- **Immutability**: Counter updates use object spread, avoiding accidental mutation of caller state.

### Issues

- **Major — side effects in “pure” presentation logic**: Every helper logs with `console.debug` / `console.info`. That breaks **purity** (same inputs can imply observable differences), couples domain rules to the console, and can add noise or cost on hot paths. Elsewhere in `tddy-web`, logging often uses a `[tddy][Component]` style (e.g. `ConnectionTerminalChrome`, `GhosttyTerminal`); here the prefix is `[terminalPresentation]`, which is **inconsistent** with that pattern.
- **Minor — `defaultTerminalMiniOverlayPlacement`**: Returns a fixed union member via a no-arg function. Reasonable if the PRD may add configuration later; otherwise a named constant would be simpler. The return type is wider than the single runtime value (acceptable for API stability).

### Refactor suggestions

- Move logging to call sites (e.g. `ConnectionScreen` / router effects) **or** centralize behind a small debug helper that matches `[tddy][terminalPresentation]` and is easy to strip or gate in production builds—without changing the mathematical core of the functions.
- Align log prefix with existing `[tddy][…]` conventions if logs stay in-module.
- If duplication between `applyOverlayPreviewClickToFull` and `applyDedicatedTerminalBackToMini` feels heavy, a single internal `withPresentation(presentation, counters)` could reduce repetition; only do this if the team prefers fewer lines over named, grep-friendly entry points.

---

## `terminalPresentation.test.ts`

### Strengths

- **Acceptance-oriented structure**: `describe` blocks read like scenarios (new vs reconnect, overlay click, back to mini, idempotent reconnect, placement). Good alignment with how PMs and reviewers read tests.
- **Coverage of critical paths**: New vs reconnect, overlay + reconnect granularity, counter preservation on overlay→full, disconnect unchanged on full→mini, attach-kind mapping, placement default.

### Issues

- **Minor — duplicated scenarios**: The same high-level flows appear again in `ConnectionScreen.test.tsx` (resume → reconnect path, new → full + push). That duplicates maintenance when rules change; it may be intentional as a thin “integration contract” for `ConnectionScreen`, but it is still duplication.
- **Minor — branch coverage gap**: `nextPresentationFromAttach` uses `shouldPushTerminalRoute = prev !== "full"` for `kind === "new"`. There is **no** test with `prev === "full"` and `kind === "new"` to assert `shouldPushTerminalRoute === false` (and presentation remains `"full"`). That branch is easy to regress.

### Refactor suggestions

- Add one focused test: `nextPresentationFromAttach("full", "new")` → `{ presentation: "full", shouldPushTerminalRoute: false }`.
- Decide explicitly: either **drop** overlapping cases from `ConnectionScreen.test.tsx` and rely on `terminalPresentation` tests plus Cypress, or **rename** ConnectionScreen tests to stress “wiring/import contract” only and keep them minimal (one smoke case).

---

## `appRoutes.ts`

### Strengths

- **Single responsibility**: URL building, deep-link alias, pathname parsing, and boolean route checks are separated. `parseTerminalSessionIdFromPathname` handles empty segment, extra slashes, and `decodeURIComponent` failures—clear edge-case handling without deep nesting.
- **Consistency**: `terminalDeepLinkSessionPath` is documented to stay aligned with `terminalPathForSessionId`; the test file encodes that invariant for encoded IDs.
- **Naming**: `TERMINAL_SESSION_ROUTE_PREFIX`, `terminalPathForSessionId`, `isSessionListPath`, `isAuthCallbackPath` are self-explanatory and consistent with a small routing module.

### Issues

- **Minor — logging only on deep link helper**: `terminalDeepLinkSessionPath` logs; sibling helpers do not. Not wrong, but asymmetric—readers may wonder why only this entry point is traced.

### Refactor suggestions

- If deep-link logging is for debugging only, mirror the same policy as `terminalPresentation` (call-site or gated logger) for consistency.
- No structural refactor needed; module is appropriately small.

---

## `ConnectionScreen.test.tsx`

### Strengths

- **Explicit scope**: Comment states that full DOM coverage lives in Cypress; this file only checks the helper contract used for branching—sets expectations correctly.
- **Imports**: Pulls from `./connection/terminalPresentation`, matching how the screen should depend on presentation logic.

### Issues

- **Minor — overlap with `terminalPresentation.test.ts`**: The two `it` blocks largely restate acceptance tests already covered in the module test file. Risk: two places to update for one rule change.

### Refactor suggestions

- Prefer **one** authoritative layer for exhaustive acceptance tests (`terminalPresentation.test.ts`) and keep `ConnectionScreen.test.tsx` to a **single** smoke test that imports `ConnectionScreen` only if/when a shallow render or prop assertion is added; otherwise consider removing duplicate cases and relying on the module + Cypress as documented.

---

## Cross-cutting (SOLID & consistency)

- **SRP**: Presentation rules are extracted from UI (`terminalPresentation`), and URL rules from navigation (`appRoutes`)—good separation. The main tension is **logging inside pure helpers**, which blends observability with domain logic.
- **Duplication**: Acceptance overlap between `ConnectionScreen.test.tsx` and `terminalPresentation.test.ts` is the main DRY concern; logging prefix inconsistency with `[tddy][…]` is a smaller consistency issue.
