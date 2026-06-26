/**
 * VNC tab content in the session inspector drawer.
 *
 * Shows a list of VNC targets configured for the session, an Add form, and
 * per-target Start/Stop/Remove buttons. Starting a target opens the VncOverlay.
 *
 * STUB — renders a placeholder panel only.
 * Full implementation comes in the green phase.
 */

import React from "react";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SessionVncTabProps {
  sessionId: string;
  sessionToken: string;
  onListVncTargets: () => Promise<VncTargetInfo[]>;
  onAddVncTarget: (req: AddVncTargetReq) => Promise<VncTargetInfo>;
  onRemoveVncTarget: (targetId: string) => Promise<void>;
  onUnlockVncVault: (passphrase: string) => Promise<void>;
  onStartVncStream: (targetId: string) => Promise<StartVncStreamResult>;
  onStopVncStream: (targetId: string) => Promise<void>;
}

export interface VncTargetInfo {
  id: string;
  label: string;
  host: string;
  port: number;
}

export interface AddVncTargetReq {
  label: string;
  host: string;
  port: number;
  password: string;
}

export interface StartVncStreamResult {
  livekitRoom: string;
  livekitUrl: string;
  bridgeIdentity: string;
  trackName: string;
  width: number;
  height: number;
}

// ---------------------------------------------------------------------------
// Component (STUB)
// ---------------------------------------------------------------------------

export function SessionVncTab(_props: SessionVncTabProps): React.ReactElement {
  // FIXME: implement in green phase
  return (
    <div data-testid="sessions-vnc-tab-panel" />
  );
}
