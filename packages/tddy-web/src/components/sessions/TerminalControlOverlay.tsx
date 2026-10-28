import { Button } from "../ui/button";
import { InlineError } from "../ui/inline-error";

export interface TerminalControlOverlayProps {
  /** Whether THIS screen currently holds the terminal control lease for the session. */
  isController: boolean;
  /** The screen id of the current lease holder (shown when this screen is not the controller). */
  holderScreenId: string;
  /** Steal control from the current holder (the "Claim terminal" action). */
  onClaim: () => void;
  /** Why the last "Claim terminal" did not take control — shown under the button. */
  claimError?: string | null;
}

/**
 * "Claim terminal" toast — rendered over a focused session runtime when this screen does NOT hold
 * the terminal control lease. It does not block the terminal: the panes stay visible behind it and
 * are greyed out by the runtime instead (see `SessionRuntime`), while this toast floats at the top
 * of the terminal area naming the holder and offering the steal-claim. Pure presentation: the lease
 * state (`isController` / `holderScreenId`) and the steal-claim action (`onClaim`) are owned by the
 * session runtime via `useTerminalControl`. When this screen IS the controller, renders nothing.
 *
 * PRD: `docs/ft/daemon/terminal-sessions.md` (control section) and
 *      `docs/ft/web/session-drawer.md` (Claim terminal CTA section).
 */
export function TerminalControlOverlay({
  isController,
  holderScreenId,
  onClaim,
  claimError = null,
}: TerminalControlOverlayProps) {
  if (isController) return null;
  return (
    <div
      data-testid="terminal-control-overlay"
      className="pointer-events-auto absolute left-1/2 top-3 z-10 flex max-w-[95%] -translate-x-1/2 flex-col items-center"
    >
      <div className="flex items-center gap-3 rounded-md border border-border bg-background/95 py-1.5 pl-3 pr-1.5 shadow-lg">
        <span className="text-xs text-muted-foreground">
          Controlled by another screen
          <span
            data-testid="terminal-control-holder"
            className="ml-2 font-mono text-[11px] text-muted-foreground/70"
          >
            {holderScreenId}
          </span>
        </span>
        <Button size="sm" data-testid="terminal-claim-btn" onClick={onClaim}>
          Claim terminal
        </Button>
      </div>
      <InlineError testId="terminal-claim-error" message={claimError} className="mt-2 max-w-md text-center" />
    </div>
  );
}
