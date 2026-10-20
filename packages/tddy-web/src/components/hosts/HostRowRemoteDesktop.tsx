/**
 * Whether a host's desktop is reachable, whether tddy could bridge it, and — where both hold and
 * the wire can carry a video track — a way in.
 *
 * Two facts, side by side and never merged:
 *
 * - **Can bridge** — this host's daemon has the VNC/RDP bridge binary.
 * - **Desktop reachable** — something is serving on the checked port.
 *
 * A host can have one without the other, and the fixes are unrelated: install the bridge, versus
 * start a desktop server. The row also names the **port that was checked**, so "unreachable" is not
 * read as authoritative on a host serving somewhere non-default.
 *
 * ⚠ The connect action needs a third fact those two cannot supply: whether this host is reached
 * over a connection that carries media. `IPC_CAPABILITIES` is `{"rpc"}` — a frame pipe carries no
 * video — so on a host reached that way a LiveKit track cannot arrive however reachable its desktop
 * is. Following `InspectorTabs`, the action is then **removed** rather than rendered disabled: a
 * control that provably cannot work is worse than no control.
 */

import { Fragment, useState } from "react";
import { DesktopChannelKind, ProbeOutcome, type HostRemoteDesktop } from "../../gen/host_pb";
import { Protocol } from "../../gen/screen_sharing_pb";
import { useHostConnection } from "../../rpc/connections/registry";
import { useHasCapability } from "../../rpc/connections/useHasCapability";
import { HostDesktopOverlay } from "./HostDesktopOverlay";

export interface HostRowRemoteDesktopProps {
  instanceId: string;
  /** One entry per probed protocol. */
  readings: readonly HostRemoteDesktop[];
}

/**
 * `screen_sharing.proto`'s `Protocol` values, which this block reuses rather than restating.
 * A reading for anything else is not rendered: there is no name to label it with.
 */
const PROTOCOL_NAMES: Record<number, { slug: "vnc" | "rdp"; label: string; protocol: Protocol }> = {
  [Protocol.VNC]: { slug: "vnc", label: "VNC", protocol: Protocol.VNC },
  [Protocol.RDP]: { slug: "rdp", label: "RDP", protocol: Protocol.RDP },
};

function isX11Display(reading: HostRemoteDesktop): boolean {
  return reading.kind === DesktopChannelKind.X11_DISPLAY;
}

/** What the desktop half of a reading says. Kept separate from the bridge half on purpose. */
function desktopFact(reading: HostRemoteDesktop): string {
  // An X11 display has no port until it is opened: the reading is about the display's socket.
  if (isX11Display(reading)) {
    return reading.outcome !== ProbeOutcome.OK
      ? "Could not check the display"
      : "Display available";
  }
  // A probe that could not run is not a negative finding. "No desktop" here would report a host we
  // never reached as one with nothing serving.
  if (reading.outcome !== ProbeOutcome.OK) {
    return `Could not check :${reading.port}`;
  }
  return reading.desktopReachable
    ? `Desktop on :${reading.port}`
    : `No desktop on :${reading.port}`;
}

/** The endpoint a connect would open, named in the vocabulary `ScreenSharingService` speaks. */
export interface ConnectableDesktop {
  protocol: Protocol;
  port: number;
  /** The reading's `channelId`, which tells two channels on one host apart. */
  channelId?: string;
  /** Set for an X11-display channel: the display the daemon serves through x11vnc. */
  x11Display?: number;
}

/**
 * The channel a reading would open, or `null` when a connect on it could not work.
 *
 * All three conditions are node 7's, and none of them is redundant. A probe that could not run made
 * no finding at all; a desktop nothing is serving has nothing to stream; and a reachable desktop on
 * a daemon with no bridge binary is a stream that would fail at spawn, after the operator asked for
 * it.
 */
function connectableChannel(reading: HostRemoteDesktop): ConnectableDesktop | null {
  const known = PROTOCOL_NAMES[reading.protocol];
  if (!known) return null;
  if (reading.outcome !== ProbeOutcome.OK) return null;
  if (!reading.desktopReachable || !reading.canBridge) return null;
  return {
    protocol: known.protocol,
    port: reading.port,
    channelId: reading.channelId,
    x11Display: isX11Display(reading) ? reading.display : undefined,
  };
}

/**
 * Every channel a connect could act on, in the probe's order — one Connect each.
 *
 * A host with several X displays offers each one.
 */
export function connectableChannels(
  readings: readonly HostRemoteDesktop[],
): ConnectableDesktop[] {
  return readings.flatMap((reading) => connectableChannel(reading) ?? []);
}

/**
 * The first desktop a connect would act on, or `null` when none of the readings is connectable —
 * the probe's own order (VNC, then RDP) rather than a preference this row invents.
 */
export function connectableDesktop(
  readings: readonly HostRemoteDesktop[],
): ConnectableDesktop | null {
  return connectableChannels(readings)[0] ?? null;
}

export function HostRowRemoteDesktop({ instanceId, readings }: HostRowRemoteDesktopProps) {
  // The one predicate every media surface is gated on — never re-derived from a `Room`, a transport
  // or a status string. See `rpc/connections/useHasCapability`.
  const connection = useHostConnection(instanceId);
  const carriesMedia = useHasCapability(connection, "media");
  const [openDesktop, setOpenDesktop] = useState<ConnectableDesktop | null>(null);

  return (
    <>
      <span
        data-testid={`hosts-row-${instanceId}-remote-desktop`}
        className="flex gap-3 text-xs text-muted-foreground"
      >
        {readings.map((reading) => {
          const protocol = PROTOCOL_NAMES[reading.protocol];
          if (!protocol) return null;
          const x11 = isX11Display(reading);
          const connectable = connectableChannel(reading);
          return (
            <Fragment key={reading.channelId || protocol.slug}>
              <span
                data-testid={
                  x11
                    ? `hosts-row-${instanceId}-channel-${reading.channelId}`
                    : `hosts-row-${instanceId}-${protocol.slug}`
                }
                className="flex gap-1"
                title={reading.failureReason || undefined}
              >
                <span className="font-medium">{x11 ? reading.label : protocol.label}</span>
                <span>{reading.canBridge ? "Bridge ready" : "No bridge"}</span>
                <span>·</span>
                <span>{desktopFact(reading)}</span>
              </span>
              {carriesMedia && connectable && (
                <button
                  data-testid={`hosts-row-${instanceId}-connect-desktop`}
                  data-channel-id={connectable.channelId}
                  type="button"
                  className="px-2 py-0.5 text-xs border border-border rounded hover:bg-accent"
                  onClick={() => setOpenDesktop(connectable)}
                >
                  Connect
                </button>
              )}
            </Fragment>
          );
        })}
      </span>
      {openDesktop && (
        <HostDesktopOverlay
          hostId={instanceId}
          port={openDesktop.port}
          protocol={openDesktop.protocol}
          x11Display={openDesktop.x11Display}
          onClose={() => setOpenDesktop(null)}
        />
      )}
    </>
  );
}
