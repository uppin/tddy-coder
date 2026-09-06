/**
 * A host's desktop, in the overlay tddy already has.
 *
 * A thin host-scoped mount of `ScreenSharingOverlay` — the same LiveKit `VideoTrack` subscription and
 * the same input forwarding the session-scoped tab uses. **No browser-side VNC/RDP client is added
 * here, ever**: rendering is always a daemon-produced video track, which is the existing
 * architecture and not a limitation to route around.
 *
 * ⚠ This surface cannot exist without the `media` capability. `IPC_CAPABILITIES` is `{"rpc"}` — a
 * frame pipe carries no video — so on the desktop build's own IPC-reached host a track cannot
 * arrive at all. Following `InspectorTabs`, the entry point is **removed** rather than shown broken.
 */

export interface HostDesktopOverlayProps {
  hostId: string;
  /** LiveKit coordinates from `StartHostStream` — the same fields the session-scoped path returns. */
  livekitRoom: string;
  livekitUrl: string;
  bridgeIdentity: string;
  trackName: string;
  width: number;
  height: number;
  onClose: () => void;
}

export function HostDesktopOverlay(props: HostDesktopOverlayProps) {
  // TODO(desktop-connect): implement — mount ScreenSharingOverlay with these coordinates.
  return <div data-testid={`host-desktop-overlay-${props.hostId}`} />;
}
