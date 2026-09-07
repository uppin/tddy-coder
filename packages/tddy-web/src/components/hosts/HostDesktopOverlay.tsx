/**
 * A host's desktop, in the overlay tddy already has.
 *
 * A thin host-scoped mount of `ScreenSharingOverlay` — the same LiveKit `VideoTrack` subscription
 * the session-scoped tab uses. **No browser-side VNC/RDP client is added here, ever**: rendering is
 * always a daemon-produced video track, which is the existing architecture and not a limitation to
 * route around.
 *
 * ⚠ This surface cannot exist without the `media` capability. `IPC_CAPABILITIES` is `{"rpc"}` — a
 * frame pipe carries no video — so on the desktop build's own IPC-reached host a track cannot
 * arrive at all. Following `InspectorTabs`, the entry point is **removed** rather than shown broken;
 * that gate is `HostRowRemoteDesktop`'s, and this component is only ever mounted past it.
 *
 * **The overlay opens on the click, not on the reply.** Starting a bridge is a spawn on a remote
 * machine, so there is a real interval between asking and a track existing — and a request that
 * fails has to say so somewhere. Rendering nothing until `StartHostStream` returns would leave the
 * operator with an action that appeared to do nothing, and no place to put the error.
 */

import { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  Protocol,
  ScreenSharingService,
  type StartStreamResponse,
} from "../../gen/screen_sharing_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useCommonRoom } from "../../hooks/useCommonRoom";
import { presenceIdentityForUser } from "../../lib/presenceIdentity";
import { useHostClient } from "../../rpc/connections/registry";
import { ScreenSharingOverlay } from "../sessions/ScreenSharingOverlay";

export interface HostDesktopOverlayProps {
  /** The daemon instance whose desktop this is — host scope's whole addressing change. */
  hostId: string;
  /** The port node 7's probe found serving. Port discovery is out of scope; this is what it checked. */
  port: number;
  /** `screen_sharing.proto`'s `Protocol`, as node 7's reading reports it. */
  protocol: Protocol;
  onClose: () => void;
}

/**
 * The address a target built from a probe reading points at.
 *
 * Node 7 probes each daemon's **own loopback** (`remote_desktop_probe.rs`'s `PROBE_HOST`), so this
 * is not a guess about where the desktop is: it is the address the reachable/unreachable answer was
 * made against. A desktop on some other machine's loopback is that daemon's reading to take.
 */
const PROBED_HOST = "127.0.0.1";

/** What a target created from a probe reading is called, so an operator can tell it apart later. */
const PROBED_TARGET_LABEL = "Desktop";

/**
 * The host-scoped target for the probed endpoint, creating one the first time.
 *
 * A row has a port and a protocol; `StartHostStream` takes a target id. Matching on the endpoint
 * rather than adding unconditionally is what stops one target accumulating per connect.
 */
async function targetForProbedEndpoint(
  client: Client<typeof ScreenSharingService>,
  hostId: string,
  port: number,
  protocol: Protocol,
): Promise<string> {
  const existing = await client.listHostTargets({ daemonInstanceId: hostId });
  const match = existing.targets.find(
    (target) =>
      target.host === PROBED_HOST && target.port === port && target.protocol === protocol,
  );
  if (match) return match.id;

  const added = await client.addHostTarget({
    daemonInstanceId: hostId,
    label: PROBED_TARGET_LABEL,
    host: PROBED_HOST,
    port,
    protocol,
    username: "",
  });
  return added.targetId;
}

export function HostDesktopOverlay({ hostId, port, protocol, onClose }: HostDesktopOverlayProps) {
  const client = useHostClient(ScreenSharingService, hostId);
  const [stream, setStream] = useState<StartStreamResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Start on mount and stop on unmount: the bridge process lives exactly as long as this overlay is
  // open, so closing it releases the process rather than leaving one per desktop an operator looked
  // at. The stop is addressed at the target the start actually used, which is why it is captured
  // here rather than re-resolved.
  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    let startedTargetId: string | null = null;

    void (async () => {
      try {
        const targetId = await targetForProbedEndpoint(client, hostId, port, protocol);
        if (cancelled) return;
        startedTargetId = targetId;
        // TODO(#hosts-screen 8/8): a desktop needing a password must be prompted for one through
        // node 6's encrypted channel and the ciphertext passed as `encryptedPassword`, never
        // persisted (AC-7). Password-less desktops connect today; a protected one is refused by the
        // daemon rather than silently retried.
        const started = await client.startHostStream({ daemonInstanceId: hostId, targetId });
        if (cancelled) return;
        setStream(started);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    })();

    return () => {
      cancelled = true;
      if (startedTargetId) {
        void client
          .stopHostStream({ daemonInstanceId: hostId, targetId: startedTargetId })
          .catch(() => {
            // The overlay is already gone; a failed stop is the daemon's to reconcile, and there is
            // no surface left to report it on.
          });
      }
    };
  }, [client, hostId, port, protocol]);

  // The bridge publishes into the room the start reply names, which is not this page's common room:
  // joining it is the same mint-and-connect `useCommonRoom` already performs, with no coordinates
  // until there is a reply.
  const { user } = useAuthContext();
  const identity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user],
  );
  const { room } = useCommonRoom(stream?.livekitUrl, stream?.livekitRoom, identity);

  // TODO(#hosts-screen 8/8): forward pointer and keyboard input to the host's bridge (AC-3). The
  // session-scoped overlay does not forward either — `vncInput.ts` is left over from a `VncOverlay`
  // that no longer exists — so this needs a host-scoped input channel rather than a reuse.

  return (
    <div data-testid={`host-desktop-overlay-${hostId}`}>
      {stream ? (
        <ScreenSharingOverlay
          room={room}
          bridgeIdentity={stream.bridgeIdentity}
          trackName={stream.trackName}
          width={stream.width}
          height={stream.height}
          onClose={onClose}
        />
      ) : (
        <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-black/90 text-sm text-white">
          <span data-testid={`host-desktop-overlay-${hostId}-status`} role="status">
            {error ?? `Connecting to ${hostId}…`}
          </span>
          <button
            data-testid={`host-desktop-overlay-${hostId}-close`}
            type="button"
            className="px-3 py-1 text-sm bg-white/10 rounded hover:bg-white/20"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      )}
    </div>
  );
}
