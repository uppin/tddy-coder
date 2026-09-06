/**
 * Whether a host's desktop is reachable, and whether tddy could bridge it.
 *
 * Two facts, side by side and never merged:
 *
 * - **Can bridge** — this host's daemon has the VNC/RDP bridge binary.
 * - **Desktop reachable** — something is serving on the checked port.
 *
 * A host can have one without the other, and the fixes are unrelated: install the bridge, versus
 * start a desktop server. The row also names the **port that was checked**, so "unreachable" is not
 * read as authoritative on a host serving somewhere non-default.
 */

import { ProbeOutcome, type HostRemoteDesktop } from "../../gen/connection_pb";

export interface HostRowRemoteDesktopProps {
  instanceId: string;
  /** One entry per probed protocol. */
  readings: readonly HostRemoteDesktop[];
}

/**
 * `screen_sharing.proto`'s `Protocol` values, which this block reuses rather than restating.
 * A reading for anything else is not rendered: there is no name to label it with.
 */
const PROTOCOL_NAMES: Record<number, { slug: "vnc" | "rdp"; label: string }> = {
  1: { slug: "vnc", label: "VNC" },
  2: { slug: "rdp", label: "RDP" },
};

/** What the desktop half of a reading says. Kept separate from the bridge half on purpose. */
function desktopFact(reading: HostRemoteDesktop): string {
  // A probe that could not run is not a negative finding. "No desktop" here would report a host we
  // never reached as one with nothing serving.
  if (reading.outcome !== ProbeOutcome.OK) {
    return `Could not check :${reading.port}`;
  }
  return reading.desktopReachable
    ? `Desktop on :${reading.port}`
    : `No desktop on :${reading.port}`;
}

export function HostRowRemoteDesktop({ instanceId, readings }: HostRowRemoteDesktopProps) {
  return (
    <span
      data-testid={`hosts-row-${instanceId}-remote-desktop`}
      className="flex gap-3 text-xs text-muted-foreground"
    >
      {readings.map((reading) => {
        const protocol = PROTOCOL_NAMES[reading.protocol];
        if (!protocol) return null;
        return (
          <span
            key={protocol.slug}
            data-testid={`hosts-row-${instanceId}-${protocol.slug}`}
            className="flex gap-1"
            title={reading.failureReason || undefined}
          >
            <span className="font-medium">{protocol.label}</span>
            <span>{reading.canBridge ? "Bridge ready" : "No bridge"}</span>
            <span>·</span>
            <span>{desktopFact(reading)}</span>
          </span>
        );
      })}
    </span>
  );
}
