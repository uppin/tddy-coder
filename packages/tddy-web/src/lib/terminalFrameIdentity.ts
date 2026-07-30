/**
 * Per-frame terminal identity guard.
 *
 * The daemon stamps every `SessionTerminalOutput` frame with the session and terminal it came from
 * (see `connection.proto`). A pane compares that stamp against the terminal it is rendering and
 * drops anything foreign, so a mis-routed subscription anywhere in the chain is caught at the write
 * boundary instead of being silently painted into the wrong terminal.
 */

/** The reserved terminal id an empty `terminal_id` resolves to — mirrors the daemon's resolution. */
export const MAIN_TERMINAL_ID = "main";

/** The session/terminal a frame was stamped with, or the pane's own target terminal. */
export interface TerminalIdentity {
  sessionId: string;
  terminalId: string;
}

/**
 * True when `frame` was produced by the terminal `pane` renders.
 *
 * The pane's `terminalId` is resolved the same way the daemon resolves the request's: an Agent pane
 * asks for the empty terminal id but its frames come back stamped with the resolved `main`. A frame
 * carrying no identity at all is foreign — it cannot be traced to a terminal, so it is dropped
 * rather than trusted.
 */
export function isFrameForTerminal(frame: TerminalIdentity, pane: TerminalIdentity): boolean {
  if (frame.sessionId === "" || frame.terminalId === "") return false;
  return frame.sessionId === pane.sessionId && frame.terminalId === resolveTerminalId(pane.terminalId);
}

/** Resolve an empty terminal id to the reserved main ("claude"/Agent) terminal. */
function resolveTerminalId(terminalId: string): string {
  return terminalId === "" ? MAIN_TERMINAL_ID : terminalId;
}
