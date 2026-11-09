# xterm.js engine

Every terminal constructs **`xtermEngine`** (`src/engine/xtermEngine.ts`), the
[`TerminalEngine`](terminal-engine.md) on xterm.js. It builds an `@xterm/xterm` `Terminal` with:

- `@xterm/addon-fit`, which proposes a grid.
- `@xterm/addon-unicode11`, with Unicode 11 widths active so emoji and CJK take two cells.
- `@xterm/xterm/css/xterm.css`, imported by the engine.

`allowProposedApi` is on because the Unicode 11 width provider is a proposed API. An option the
caller leaves out (`fontFamily`, `theme`, `scrollback`) keeps xterm's default. It is not passed as
`undefined`, which xterm rejects.

xterm.js appends a root element (`.xterm`) inside the container. The grid is drawn in
`.xterm-screen`, which `screenElement` returns, and which the touch routes and the fixed-grid font
fit measure. `dispose()` disposes the terminal and its addons and removes that root element.

## Renderer

After `open`, the engine loads `@xterm/addon-webgl`.

- **No WebGL context:** the addon throws on load, and xterm keeps its DOM renderer.
- **Context lost for good:** the addon first waits three seconds for a restore, then fires
  `onContextLoss`. The engine disposes the addon, and xterm switches back to its DOM renderer.

The active renderer is named on xterm's root element as `data-terminal-renderer` (`webgl` | `dom`).

`createXtermEngine(options, createWebgl)` takes the WebGL addon's factory. Headless browsers have no
WebGL context, so `TerminalRenderer.cy.tsx` passes a stand-in that can lose its context on demand.

## Write parsing

xterm.js parses writes asynchronously, and `xtermEngine` forwards its `onWriteParsed`. The
components depend on it in four places:

- **Buffer text:** the `getBufferText()` memo revision moves on every parse, so a read between a
  write and its parse is not cached.
- **Ready:** `onReady` (and `data-terminal-ready`) fires only once `initialContent` has been parsed.
- **Live pane modes:** after a mode-switching chunk, the session's live pane re-reads its DEC modes
  (`tuiOwnsNavigation`) on every parse.
- **Dormant agent tab:** it reads the recorded output's text once the latest write is parsed.

## What xterm.js owns

xterm.js handles these itself, and no component listener duplicates them:

- Mouse and wheel reporting to a mouse-tracking program: one SGR report per click and per wheel step.
- Arrow keys for a wheel step on the alternate screen without mouse tracking.
- Scrolling the normal screen.
- The device pixel ratio.
- The buffer-change event.

The components keep only the input a browser gives xterm no event for:

- **Touch drag** (`useTerminalTouchScroll`): sends an SGR wheel report per line of finger travel to a
  mouse-tracking TUI.
- **Tap as click** (`useTapAsClick`): re-synthesises the mouse events that a cancelled `touchstart`
  withholds, and xterm reports them. A drag or a second finger cancels the tap, so it sends no press.

A trackpad pinch (`wheel` with `ctrlKey`) is stopped in the capture phase on the container. It zooms
the font and reaches neither xterm's scrolling nor a TUI.

## Kept around xterm.js

Each of these covers something the library does not (developer decisions, 2026-11-08):

- **OSC 52:** handled by the components' write tap (`lib/osc52.ts`), not `@xterm/addon-clipboard`.
  The tap answers no clipboard reads, caps the payload size, and falls back where
  `navigator.clipboard` is absent. The addon would copy a second time and answer reads.
- **IME Escape forward** (`useImeEscapeForwarding`): xterm's `CompositionHelper` also drops a
  keydown with `keyCode` 229, so Escape pressed during composition never reaches the program
  otherwise.
- **`lib/terminalDprWatch.ts`:** stays while `ghosttyEngine` uses it.
- **Write buffering in `useFeedStream`:** covers data that arrives before the engine is attached, not
  only parse delay.

## Specs

| Spec | What it checks |
|---|---|
| `TerminalEngineContract.cy.tsx` | The contract suite, run against both engines |
| `TerminalMouseReporting.cy.tsx` | One SGR report per click, per line-mode wheel step and per pixel-mode wheel notch |
| `TerminalUnicodeWidth.cy.tsx` | Wide characters take two cells |
| `TerminalRenderer.cy.tsx` | The renderer is named; DOM without WebGL; DOM after a context loss |
