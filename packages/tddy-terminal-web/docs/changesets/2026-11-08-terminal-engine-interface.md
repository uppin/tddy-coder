# 2026-11-08 — Terminals run on a TerminalEngine interface with top-based scrolling (#xterm 2/4, PR #867)

**Type:** Refactor

- `src/engine/TerminalEngine.ts` defines the interface; `src/engine/ghosttyEngine.ts` implements it
  on ghostty-web and is, with `lib/ghosttyInstance.ts`, the only module that imports the library or
  casts into it (`src/engine/engineBoundary.test.ts`).
- Scroll positions are top-based in the engine, the imperative handle (`getViewportScrollOffset` →
  `getViewportTop`), `lib/terminalScrollbar.ts` (`viewportY` → `viewportTop`) and the history
  paging driver. The `terminal-page-viewport-y` / `terminal-live-viewport-y` probes are gone; specs
  read the `total,offset,len` scrollbar mirrors.
- The hooks no longer hold a `FitAddon`: fitting is `fitTerminalToContainer` (`proposeDimensions` +
  `resize`), driven by the lifecycle's ResizeObserver on the container and by font-size changes.
- `getBufferText` pads each trimmed engine line to `cols` before joining.
- `src/engine/terminalEngineContract.ts` is a reusable contract suite, run against `ghosttyEngine`
  by `cypress/component/TerminalEngineContract.cy.tsx` (output, events, geometry, top-based
  scrolling, modes, lifecycle).
- Docs: [terminal-engine.md](../terminal-engine.md).
