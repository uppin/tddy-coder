import React from "react";
import { Minimize2 } from "lucide-react";
import { TerminalControlOverlay, type TerminalControlOverlayProps } from "./TerminalControlOverlay";

/**
 * The two pieces of chrome a pane stack carries over its terminals, shared by the live runtime
 * (`SessionRuntime`) and a stopped session's strip (`DormantSessionRuntime`) so the two cannot drift.
 */

/**
 * The way back out of full screen. The tab strip — and with it the strip's own toggle — is outside
 * the fullscreen element, so full screen needs its own way back; rendered only by the stack that
 * actually holds fullscreen. Esc still works; this is the in-app equivalent.
 */
export function PaneFullscreenExit({ onExit }: { onExit: () => void }): React.ReactElement {
  return (
    <button
      type="button"
      data-testid="sessions-terminal-fullscreen-exit"
      aria-label="Exit full screen"
      title="Exit full screen"
      onClick={onExit}
      className="absolute right-2 top-2 z-20 rounded border border-border bg-background/80 p-1.5 text-muted-foreground opacity-60 transition-opacity hover:opacity-100 hover:text-foreground"
    >
      <Minimize2 className="h-3.5 w-3.5" />
    </button>
  );
}

/**
 * The container the "Claim terminal" toast floats in. `pointer-events-none` lets clicks reach the
 * terminal below; only the toast re-enables them. Its position and stacking are stated inline because
 * `terminal-live-pane` claims `position: absolute; z-index: 2` inline, so a toast that only claimed
 * them through utility classes would be painted over by the very canvas it floats over.
 *
 * The ladder in a pane stack, bottom up: terminal panes (2, greyed out while controlled elsewhere),
 * this toast (3), an open agent conversation (4).
 */
export function TerminalControlLayer({
  testId,
  overlay,
}: {
  testId: string;
  /** What the toast says, or `null` while the lease is not being managed — the container is still
   *  there (it is an acceptance marker) but holds no toast. */
  overlay: TerminalControlOverlayProps | null;
}): React.ReactElement {
  return (
    <div
      data-testid={testId}
      className="absolute inset-0 pointer-events-none"
      style={{ position: "absolute", inset: 0, zIndex: 3 }}
    >
      {overlay && <TerminalControlOverlay {...overlay} />}
    </div>
  );
}
