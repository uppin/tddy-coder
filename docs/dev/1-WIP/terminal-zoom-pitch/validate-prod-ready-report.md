# Terminal zoom — production readiness validation

**Scope:** `terminalZoom.ts`, `terminalZoomBridge.ts`, `TerminalZoomToolbar.tsx`, `GhosttyTerminal.tsx` (zoom bridge + `applyFontSizePx`), `GhosttyTerminalLiveKit.tsx`, `ConnectionTerminalChrome.tsx` (toolbar baseline).

---

## Summary

The zoom feature is structurally sound: bounds live in `terminalZoom.ts`, the toolbar and terminal communicate via window `CustomEvent`s, and `GhosttyTerminal` ignores bridge actions when `termRef` is missing (with an explicit log). **The main production gaps are logging volume** (`console.debug` / `console.info` on hot paths, including every `canPitchIn` / `canPitchOut` evaluation during render) **and missing defensive validation** of event `detail` and numeric font values (NaN / missing detail). **Configuration** is mostly centralized (8–32 defaults); **baseline 14** is duplicated in a few places but matches `GhosttyTerminal`’s default `fontSize` when the LiveKit path does not pass `fontSize`. **Security** risk from `CustomEvent` injection is low for typical same-origin apps but worth documenting. **Performance** is acceptable for normal use; `fit()` runs per font change (expected).

---

## Risk areas

| Area | Risk |
|------|------|
| Logging | High-volume `console.debug` / `console.info` on zoom and clamp paths; noisy in production and may surface internal dimensions in Ghostty logs. |
| Event detail | `TerminalZoomToolbar` sync handler assumes `ce.detail.fontSize` exists; malformed events could throw. |
| Numeric edge cases | No explicit guards against `NaN` / non-finite font sizes propagating through pitch/clamp. |
| Global events | Fixed event names on `window` — any same-origin code can dispatch; bridge handler merges opts from `detail` (bounds override). |
| Duplicated baseline `14` | LiveKit and chrome default to 14; drift if `GhosttyTerminal` `fontSize` is ever passed without updating toolbar/chrome. |

---

## Findings

### High

- **H1 — Logging on every zoom helper and toolbar render path:** `terminalZoom.ts` calls `console.debug` inside `clampTerminalFontSize`, `pitchInFontSize`, `pitchOutFontSize`, `canPitchIn`, and `canPitchOut`. `TerminalZoomToolbar` invokes `canPitchIn` / `canPitchOut` on **every render** to compute disabled state, so **each re-render emits multiple debug logs** even without user interaction. `terminalZoomBridge.dispatchTerminalZoomBridge` uses `console.info` on **every** toolbar click. `GhosttyTerminal.applyFontSizePx` uses `console.info` on **every** font application. This is likely **unsuitable for production** without gating or removal.

### Medium

- **M1 — Sync event handler assumes `detail` shape:** In `TerminalZoomToolbar`, `onSync` does `setLiveFontSize(ce.detail.fontSize)` with no check that `ce.detail` exists or `fontSize` is a finite number. A buggy emitter or hostile same-origin script could pass `undefined` or non-numeric values.

- **M2 — No validation of bridge `detail` in `GhosttyTerminal`:** The bridge handler reads `ce.detail` without verifying `action`, `baselineFontSize`, or merged bounds. Invalid `action` falls through to `pitchOutFontSize` (last branch). Unexpected types could yield `NaN` in `term.options.fontSize` if the terminal ever held a bad value.

### Low

- **L1 — CustomEvent “injection”:** Same-origin JavaScript can `dispatchEvent` `tddy-terminal-zoom` with arbitrary `detail`. Impact is limited to changing terminal font size within the page (no HTML injection in this path). Treat as **integrity of UI state**, not XSS, unless other listeners are added later.

- **L2 — `fit()` frequency:** Each zoom applies `fitAddon.fit()` once. Rapid clicks cause repeated reflow; acceptable for a manual control, but not debounced.

- **L3 — Window listener churn:** `GhosttyTerminal` registers `TERMINAL_ZOOM_BRIDGE_EVENT` for the component lifetime; `TerminalZoomToolbar` registers font sync once. Pattern is standard; no obvious leak if cleanup runs on unmount.

- **L4 — Hardcoded baseline 14:** `GhosttyTerminalLiveKit` uses `<TerminalZoomToolbar baselineFontSize={14} />` when `!connectionOverlay`. `ConnectionTerminalChrome` defaults `terminalBaselineFontSize = 14`. `GhosttyTerminal` in that tree does not set `fontSize` prop, so default 14 applies — **consistent today**. Risk is **future drift** if `fontSize` is customized in one place only.

- **L5 — `ConnectionTerminalChrome` / `GhosttyTerminalLiveKit` non-zoom logging:** Unconditional `console.log` for LiveKit and `[terminal→server]` (including byte arrays) is **outside strict zoom scope** but increases PII / content exposure in devtools when those code paths run.

---

## Recommendations

1. **Gate or strip production logging** for zoom: remove `console.debug` / `console.info` from `terminalZoom.ts` hot functions, from `dispatchTerminalZoomBridge`, from `applyFontSizePx`, or guard behind `import.meta.env.DEV` / an explicit `debugTerminalZoom` flag (single switch). Prefer not logging terminal `cols`/`rows` in production info logs.

2. **Harden event handlers:** In `TerminalZoomToolbar` sync handler, validate `typeof ce.detail?.fontSize === "number" && Number.isFinite(...)` before `setLiveFontSize`. In `GhosttyTerminal` bridge handler, validate `d` and `d.action` against a narrow union; ignore or log-once on invalid payloads.

3. **Sanitize numeric inputs:** Before applying, reject non-finite `px` / `current` (e.g. clamp only after `Number.isFinite` check, or fall back to `baselineFontSize`).

4. **Single source of baseline:** Export a named constant (e.g. `DEFAULT_TERMINAL_FONT_SIZE = 14`) used by `GhosttyTerminal` default, `TerminalZoomToolbar` baseline in LiveKit, and `ConnectionTerminalChrome` default prop to avoid silent mismatch.

5. **Security note (documentation):** Document that zoom events are **same-origin coordination** only; do not use `detail` for trusted authorization.

---

## Build note

`./dev cargo check -q` was run from the workspace root (2026-04-04). Exit code **0** (Rust workspace compiles). Nix dev shell printed a dirty-tree warning and an ignorable SQLite eval-cache busy message; they did not fail the check.
