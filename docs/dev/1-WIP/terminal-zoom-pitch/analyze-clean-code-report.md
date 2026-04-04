# Terminal zoom — clean code analysis

**Scope:** `terminalZoom.ts`, `terminalZoomBridge.ts`, `TerminalZoomToolbar.tsx`, zoom-related parts of `GhosttyTerminal.tsx`, and zoom integration in `GhosttyTerminalLiveKit.tsx` / `ConnectionTerminalChrome.tsx`.

---

## Summary

The feature splits responsibilities sensibly: **pure zoom math** in `terminalZoom.ts`, a **thin event contract** in `terminalZoomBridge.ts`, a **presentational toolbar** in `TerminalZoomToolbar.tsx`, and **application logic** in `GhosttyTerminal` (`applyFontSizePx`, bridge listener, font-size sync dispatch). The main architectural tradeoff is **implicit coupling via `window` `CustomEvent`s** instead of React context or a ref/callback — documented in the toolbar, but it complicates multi-instance scenarios and testing.

The largest quality gaps are **logging verbosity** (`console.debug` / `console.info` on hot paths in pure utilities and in `applyFontSizePx`), **minor duplication** in pitch helpers, and **magic number `14`** duplicated as baseline font in LiveKit vs chrome (partially mitigated by `terminalBaselineFontSize` on chrome).

---

## Strengths

1. **Separation of concerns** — Bounds and stepping live in testable pure functions (`terminalZoom.ts` + `terminalZoom.test.ts`). UI does not embed math.

2. **Stable bridge API** — `TerminalZoomBridgeDetail`, `TerminalZoomBridgeAction`, and event name constants are centralized in `terminalZoomBridge.ts`, reducing stringly-typed drift.

3. **Toolbar UX** — Disabled states derive from `canPitchIn` / `canPitchOut`; sync with actual terminal font uses `TERMINAL_FONT_SIZE_SYNC_EVENT` so the toolbar stays consistent after imperative changes (`setTerminalFontSize`).

4. **Integration clarity in LiveKit** — Comment explains why the toolbar is **not** rendered twice: `TerminalZoomToolbar` is omitted when `connectionOverlay` is set, because `ConnectionTerminalChrome` already embeds it.

5. **Chrome baseline** — `ConnectionTerminalChrome` exposes `terminalBaselineFontSize` (default `14`) so reset matches the terminal’s initial font when callers customize `GhosttyTerminal` `fontSize`.

6. **Accessibility** — Toolbar uses `role="toolbar"`, `aria-label` on buttons, and sensible `data-testid`s for Cypress.

---

## Issues

### Naming

- **“Pitch”** (`pitchIn` / `pitchOut`) is consistent internally but nonstandard next to common UI terms (“zoom in/out”). Acceptable if product language standardizes on “pitch.”
- **`baselineFontSize`** in bridge detail is clear for `"reset"`; the toolbar passes the same prop for all actions — correct, but the name reads strongest on reset.

### Complexity

- **`GhosttyTerminal`** remains a large component (~488 lines). Zoom adds `applyFontSizePx`, a bridge `useEffect`, and sync dispatch — each is small, but the file’s overall cyclomatic complexity is high (pre-existing: mouse/SGR, focus prevention, buffer APIs).
- **Bridge handler** uses a linear `if` chain (`reset` → `pitch-in` → else `pitch-out`). Readable; a `switch` would be equivalent.

### Duplication

- **`pitchInFontSize` / `pitchOutFontSize`** repeat the same `min` / `max` / `step` resolution from `TerminalZoomStepOptions`.
- **Toolbar** duplicates button styling for disabled vs enabled (opacity/cursor) — could be a small inner component or shared style helper.

### SOLID and coupling (`window` events)

- **Single global channel** — Any `GhosttyTerminal` instance on the page listens to `TERMINAL_ZOOM_BRIDGE_EVENT`. Multiple terminals would all react to one toolbar unless namespaced (e.g. event detail `instanceId` + listener filter) or replaced with context/refs.
- **Dispatchers are decoupled from subscribers** — Good for avoiding prop drilling; bad for **traceability** and **type-safe wiring** (no compile-time link between toolbar and terminal).
- **Open/closed** — New actions require updating `TerminalZoomBridgeAction`, `GhosttyTerminal` handler, and toolbar buttons together (no plugin shape).

### Documentation / comments

- **Toolbar** — Good high-level JSDoc explaining the window-event rationale.
- **`terminalZoom.ts`** — File-level comment is minimal; functions have one-line JSDoc. **Verbose `console.debug` on every call** acts as runtime “documentation” but is inappropriate for production noise (see below).

### Consistency with project patterns

- **Logging** — Heavy use of `[tddy][...]` prefixes aligns with other touched files, but **`terminalZoom.ts` logs inside pure functions** on every invocation; that is atypical for small math utilities and may conflict with expectations for quiet libraries.
- **SSR** — Bridge dispatch guards `typeof window === "undefined"` — consistent and correct.

### GhosttyTerminal (zoom-only)

- **`applyFontSizePx`** — Central place for clamp, `term.options.fontSize`, `fit()`, React state, and `dispatchTerminalFontSizeSync` — cohesive.
- **`useEffect` for `fontSize` prop** — Re-applies when `fontSize` / bounds change after `ready`; works with zoom bridge and external control.
- **`useImperativeHandle` `setTerminalFontSize`** — Uses same `applyFontSizePx` as bridge — good DRY.

### GhosttyTerminalLiveKit / ConnectionTerminalChrome

- **LiveKit** — When `!connectionOverlay`, renders `<TerminalZoomToolbar baselineFontSize={14} />` inline. **Hardcoded `14`** duplicates `GhosttyTerminal` default; if defaults diverge, reset will be wrong until both are updated.
- **Chrome** — Renders `<TerminalZoomToolbar baselineFontSize={terminalBaselineFontSize} />` with documented prop; fullscreen/zoom layout coexist without extra coupling beyond absolute positioning (toolbar `top: 40` vs chrome buttons `top: 8` — intentional stacking).

---

## Refactoring suggestions

1. **Reduce logging in `terminalZoom.ts`** — Remove or gate `console.debug` behind an explicit debug flag, or rely on unit tests only. Avoid logging inside `clampTerminalFontSize` when called from pitch helpers (duplicate logs).

2. **Extract `resolveStepOpts(opts)`** — Single helper for `min` / `max` / `step` defaults to dedupe `pitchInFontSize` / `pitchOutFontSize` / optionally `canPitchIn` / `canPitchOut`.

3. **Type the sync listener** — In `TerminalZoomToolbar`, import `TerminalFontSizeSyncDetail` (or reuse the bridge type) instead of `CustomEvent<{ fontSize: number }>` for consistency.

4. **Multi-terminal safety (if needed)** — Add an optional `zoomChannelId` (or React context) to event names or `detail` so only the intended `GhosttyTerminal` reacts. Skip if product guarantees a single terminal view.

5. **Optional hook** — `useTerminalZoomBridge({ termRef, min, max, applyFontSize })` could shrink `GhosttyTerminal` and centralize subscribe/unsubscribe (improves testability of the listener in isolation).

6. **Single source for baseline** — Pass `baselineFontSize` from LiveKit parent from the same constant or `GhosttyTerminal` default prop object so `GhosttyTerminal` and `TerminalZoomToolbar` cannot drift.

---

## Metrics (approximate lines per module)

| Module | Lines (wc -l) | Notes |
|--------|---------------|--------|
| `terminalZoom.ts` | 74 | Pure logic + heavy debug logs |
| `terminalZoomBridge.ts` | 45 | Types + 2 dispatchers |
| `TerminalZoomToolbar.tsx` | 126 | UI + sync listener |
| `GhosttyTerminal.tsx` | 488 | Full file; zoom-related ~90–110 lines (imports, `applyFontSizePx`, prop effect, bridge effect, imperative handle, `data-terminal-font-size`) |
| `GhosttyTerminalLiveKit.tsx` | 607 | Full file; zoom ~10 lines (import, conditional toolbar, baseline `14`) |
| `ConnectionTerminalChrome.tsx` | 282 | Full file; zoom ~5 lines (import, prop default, toolbar JSX) |

---

## Conclusion

The zoom feature is **structurally sound** (pure core, explicit bridge module, UI separated) with **clear integration documentation** in the toolbar. Main improvements: **quieter pure layer**, **slightly less duplication**, and **explicit baseline/font default sharing** between LiveKit and `GhosttyTerminal`. The **`window` event bus** is a deliberate tradeoff; accept it for simplicity or tighten it if multiple terminals or stricter coupling become requirements.
