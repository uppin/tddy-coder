import { Button } from "../ui/button";

export interface TerminalControlOverlayProps {
  /** Whether THIS screen currently holds the terminal control lease for the session. */
  isController: boolean;
  /** The screen id of the current lease holder (shown when this screen is not the controller). */
  holderScreenId: string;
  /** Steal control from the current holder (the "Claim terminal" action). */
  onClaim: () => void;
}

/**
 * "Claim terminal" mutex overlay — rendered over a focused session runtime when this screen does
 * NOT hold the terminal control lease. Pure presentation: the lease state (`isController` /
 * `holderScreenId`) and the steal-claim action (`onClaim`) are owned by the session runtime via
 * `useTerminalControl`. When this screen IS the controller, renders nothing.
 *
 * PRD: `docs/ft/daemon/terminal-sessions.md` (control section) and
 *      `docs/ft/web/session-drawer.md` (Claim terminal CTA section).
 */
export function TerminalControlOverlay({
  isController,
  holderScreenId,
  onClaim,
}: TerminalControlOverlayProps) {
  if (isController) return null;
  return (
    <div
      data-testid="terminal-control-overlay"
      className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-background/80 backdrop-blur-sm pointer-events-auto"
    >
      <p className="text-sm text-muted-foreground mb-1">Controlled by another screen</p>
      <p data-testid="terminal-control-holder" className="text-xs text-muted-foreground mb-4 font-mono">
        {holderScreenId}
      </p>
      <Button data-testid="terminal-claim-btn" onClick={onClaim}>
        Claim terminal
      </Button>
    </div>
  );
}
