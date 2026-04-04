# Terminal zoom — refactoring plan (validate orchestration)

Synthesized from `validate-tests-report.md`, `validate-prod-ready-report.md`, and `analyze-clean-code-report.md`.

## Priority 1 — Production hygiene

1. **Reduce logging noise** — Gate `console.debug` / `console.info` in `terminalZoom.ts`, `terminalZoomBridge.ts`, `GhosttyTerminal.applyFontSizePx`, and toolbar paths behind `debugLogging` or a single `TERMINAL_ZOOM_DEBUG` flag; remove per-render logging from pure helpers used in `canPitchIn`/`canPitchOut` during render.
2. **Harden event handlers** — Validate `CustomEvent` `detail` in `TerminalZoomToolbar` (sync) and `GhosttyTerminal` (bridge): finite numbers, known `action`, safe `baselineFontSize`.
3. **Delete artifact** — Remove `packages/tddy-web/.tddy-red-test-output.txt` from the worktree; gitignore if regenerated.

## Priority 2 — UX / PRD gaps

4. **Keyboard zoom** — Add document-level handlers for Ctrl/Cmd +/-/0 (and match toolbar bounds) per PRD accessibility.
5. **Baseline drift** — Thread a single `baselineFontSize` from `GhosttyTerminal` props into `GhosttyTerminalLiveKit` toolbar instead of hardcoded `14` when callers customize `fontSize`.

## Priority 3 — Architecture (optional)

6. **Replace window coupling** — Consider React context or a ref callback for multi-terminal safety; keep window events only if single-instance is guaranteed.
7. **Resize coalescing** — Only if duplicate OSC resizes cause backend issues; measure first.

## Tests

- Current: 20 passing (lib + connection unit + TerminalZoom acceptance). Add targeted tests if validation/guards for `detail` are added.
