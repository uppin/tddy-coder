/**
 * Full-screen VNC desktop overlay.
 *
 * Subscribes to the bridge participant's video track via the session's LiveKit room
 * and renders it as a full-screen overlay (`fixed inset-0 z-50`). Captures pointer
 * and keyboard events and forwards them over `VncInputService.StreamInput`.
 *
 * STUB — renders null.
 * Full implementation comes in the green phase.
 */

import React from "react";
import type { Room } from "livekit-client";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface VncOverlayProps {
  /** Session's LiveKit room (for subscribing to the bridge's video track). */
  room: Room | null;
  /** LiveKit identity of the bridge participant. */
  bridgeIdentity: string;
  /** Name of the video track to subscribe to (e.g. `vnc:<target_id>`). */
  trackName: string;
  /** Framebuffer dimensions in pixels. */
  width: number;
  height: number;
  /** Called when the user dismisses the overlay (Escape or overlay click). */
  onClose: () => void;
}

// ---------------------------------------------------------------------------
// Component (STUB)
// ---------------------------------------------------------------------------

export function VncOverlay(_props: VncOverlayProps): React.ReactElement | null {
  // FIXME: implement in green phase
  return null;
}
