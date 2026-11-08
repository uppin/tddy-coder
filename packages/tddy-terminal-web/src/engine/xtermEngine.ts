/**
 * `TerminalEngine` on xterm.js: WebGL renderer where the browser offers a WebGL context, xterm's
 * DOM renderer otherwise (developer-consented fallback, 2026-11-08). The active renderer is named
 * on the engine's container as `data-terminal-renderer` (`webgl` | `dom`).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-08-xterm-terminal-engine.md
 */

import type { TerminalEngine, TerminalEngineFactory, TerminalEngineOptions } from "./TerminalEngine";

export const xtermEngine: TerminalEngineFactory = (options: TerminalEngineOptions): TerminalEngine => {
  void options;
  // TODO(xterm-engine): implement
  throw new Error("TODO(xterm-engine): implement");
};
