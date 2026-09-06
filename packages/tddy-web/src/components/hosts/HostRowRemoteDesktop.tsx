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

import type { HostRemoteDesktop } from "../../gen/connection_pb";

export interface HostRowRemoteDesktopProps {
  instanceId: string;
  /** One entry per probed protocol. */
  readings: readonly HostRemoteDesktop[];
}

export function HostRowRemoteDesktop({ instanceId, readings }: HostRowRemoteDesktopProps) {
  // TODO(desktop-probe): implement
  void readings;
  return <span data-testid={`hosts-row-${instanceId}-remote-desktop`} />;
}
