# Terminal engine

`TerminalView` and its hooks drive the emulator only through `TerminalEngine`
(`src/engine/TerminalEngine.ts`). `xtermEngine` (`src/engine/xtermEngine.ts`) implements it on
xterm.js; it is the only module that imports xterm.js or casts
into it, which `src/engine/engineBoundary.test.ts` enforces. What xterm.js itself owns (renderer,
mouse and wheel reporting, device pixel ratio) and what stays around it: [xterm-engine.md](xterm-engine.md).

## Coordinates

Every scroll position above the engine counts **from the top** of the buffer: line 0 is the oldest
retained scrollback line, and the viewport is at the live tip when
`viewportTop() === scrollbackLength()`. xterm.js already counts from the top (`viewportY`,
`baseY`), so `xtermEngine` maps these positions straight through, including in its `onScroll`
events and `scrollToLine`.

The imperative handle follows the engine: `getViewportTop()`, `scrollToLine(line)` (top-based),
and `getScrollbar()` → `{ total, offset, len }` from `computeScrollbar` (`lib/terminalScrollbar.ts`),
with `offset` the clamped `viewportTop`.

## Fitting

The components size the engine themselves. `fitTerminalToContainer` (`src/terminal/terminalGeometry.ts`)
resizes to `proposeDimensions()` when it differs from the current grid. It runs at open, from the
lifecycle's ResizeObserver on the container (retried for up to 30 frames while the engine cannot
measure), and after every font-size change. xterm.js's `FitAddon` is used only inside the engine
to propose a grid; the components own the resize, so a throttled, occluded window cannot stall it
behind an addon timer.

xterm.js opens a root element of its own inside the container it is given; the ResizeObserver
watches the container. `dispose()` disposes the terminal (and its addons) and removes that root
element; the container belongs to the caller.

## Buffer text

`readLines()` returns each line trimmed of trailing whitespace, with whether any cell is in inverse
video. The handle's `getBufferText()` pads each line to `cols` UTF-16 units and joins them, so line
boundaries keep their place in the mirror text (`terminal-buffer-text`). Every consumer matches
substrings or regexes within a line, or strips whitespace first; none reads per-line offsets. A line
holding a wide character therefore carries one extra space of padding compared with a cell count;
no consumer can observe it.

## Write parsing

`onWriteParsed()` fires after each `write`'s data has been parsed into the buffer, so a listener
that reads `readLines()` sees what was written. xterm.js parses asynchronously, so `xtermEngine`
forwards xterm's own `onWriteParsed` event rather than firing after `write` returns.

## Contract suite

`describeTerminalEngine(name, factory)` (`src/engine/terminalEngineContract.ts`) is the behaviour any
engine must meet: output read-back, write-parsed events and inverse cells, input/bell/title events, geometry and fit
proposals, top-based scrolling, alternate-screen and mouse-tracking modes, and disposal. It needs a
browser (WASM, canvas), so it runs as a Cypress component spec:
`cypress/component/TerminalEngineContract.cy.tsx`.
