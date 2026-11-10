/**
 * Full-screen VNC desktop overlay.
 *
 * Subscribes to the bridge participant's video track via the session's LiveKit room
 * and renders it as a full-screen overlay. It forwards no input: pointer and keyboard capture
 * belongs to `ScreenSharingOverlay`, the surviving overlay, and never existed here.
 *
 * Dismiss via close button, Escape key, or clicking the backdrop.
 */

import React, { useEffect } from "react";
import { useMediaPort, type RoomHandle } from "tddy-client-web";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface VncOverlayProps {
  /** Session's LiveKit room (for subscribing to the bridge's video track). */
  room: RoomHandle | null;
  /** LiveKit identity of the bridge participant. */
  bridgeIdentity: string;
  /** Name of the video track to subscribe to (e.g. `vnc:<target_id>`). */
  trackName: string;
  /** Framebuffer dimensions in pixels. */
  width: number;
  height: number;
  /** Called when the user dismisses the overlay. */
  onClose: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function VncOverlay({
  room,
  bridgeIdentity,
  trackName,
  onClose,
}: VncOverlayProps): React.ReactElement {
  const { RemoteVideo } = useMediaPort();

  // Escape key dismisses the overlay.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      data-testid="vnc-overlay"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="relative w-full h-full flex items-center justify-center"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <button
          data-testid="vnc-overlay-close"
          type="button"
          className="absolute top-4 right-4 z-10 px-3 py-1 text-sm bg-black/60 text-white rounded hover:bg-black/80"
          onClick={onClose}
        >
          Close
        </button>
        <RemoteVideo
          testId="vnc-overlay-video"
          room={room}
          identity={bridgeIdentity}
          trackName={trackName}
          className="max-w-full max-h-full"
        />
      </div>
    </div>
  );
}
