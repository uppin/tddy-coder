# Terminal engine

`GhosttyTerminal` and its hooks drive the emulator only through `TerminalEngine`
(`src/engine/TerminalEngine.ts`). `ghosttyEngine` (`src/engine/ghosttyEngine.ts`) implements it on
ghostty-web; it and `lib/ghosttyInstance.ts` are the only modules that import ghostty-web or cast
into it, which `src/engine/engineBoundary.test.ts` enforces.

## Coordinates

Every scroll position above the engine counts **from the top** of the buffer: line 0 is the oldest
retained scrollback line, and the viewport is at the live tip when
`viewportTop() === scrollbackLength()`. ghostty-web counts from the bottom (`viewportY`, 0 = live
tip); `ghosttyEngine` translates (`viewportTop = scrollbackLength - viewportY`), including in its
`onScroll` events and `scrollToLine`.

The imperative handle follows the engine: `getViewportTop()`, `scrollToLine(line)` (top-based),
and `getScrollbar()` → `{ total, offset, len }` from `computeScrollbar` (`lib/terminalScrollbar.ts`),
with `offset` the clamped `viewportTop`.

## Fitting

The components size the engine themselves. `fitTerminalToContainer` (`src/terminal/terminalGeometry.ts`)
resizes to `proposeDimensions()` when it differs from the current grid. It runs at open, from the
lifecycle's ResizeObserver on the container (retried for up to 30 frames while the engine cannot
measure), and after every font-size change. ghostty-web's `FitAddon` is used only inside the engine
to propose a grid: its own `fit()` and `observeResize()` debounce through timers and a `_isResizing`
latch that a throttled, occluded window never clears.

ghostty-web opens on the container it is given (its `element` *is* that container), so the
ResizeObserver watches the same element the addon's observer did. `dispose()` removes only what the
engine appended (canvas, textarea); the container belongs to the caller.

## Buffer text

`readLines()` returns each line trimmed of trailing whitespace, with whether any cell is in inverse
video. The handle's `getBufferText()` pads each line to `cols` UTF-16 units and joins them, so line
boundaries keep their place in the mirror text (`terminal-buffer-text`). Every consumer matches
substrings or regexes within a line, or strips whitespace first; none reads per-line offsets. A line
holding a wide character therefore carries one extra space of padding compared with a cell count;
no consumer can observe it.

## Contract suite

`describeTerminalEngine(name, factory)` (`src/engine/terminalEngineContract.ts`) is the behaviour any
engine must meet: output read-back and inverse cells, input/bell/title events, geometry and fit
proposals, top-based scrolling, alternate-screen and mouse-tracking modes, and disposal. It needs a
browser (WASM, canvas), so it runs as a Cypress component spec:
`cypress/component/TerminalEngineContract.cy.tsx`.
