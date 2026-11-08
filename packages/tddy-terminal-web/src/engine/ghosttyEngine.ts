/**
 * `TerminalEngine` on ghostty-web. Holds every ghostty-web cast and the translation of ghostty's
 * bottom-based viewport (0 = live tip) into the interface's top-based positions.
 */

import type { TerminalEngine, TerminalEngineFactory, TerminalEngineOptions } from "./TerminalEngine";

export const ghosttyEngine: TerminalEngineFactory = (
  options: TerminalEngineOptions,
): TerminalEngine => {
  void options;
  // TODO(terminal-engine): implement
  throw new Error("TODO(terminal-engine): implement");
};
