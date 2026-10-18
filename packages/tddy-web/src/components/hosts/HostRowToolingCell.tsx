/**
 * The tooling section of one Hosts-screen row, fed by that host's own `GetHostTooling`.
 *
 * `HostRowTooling` renders a report it is handed and fetches nothing; this cell is what asks. One
 * `GetHostTooling` per **online** row per visit, addressed to the host through
 * `useHostClient(HostService, instanceId)`. An offline host has nothing to answer through, so it is
 * asked nothing.
 *
 * | Row state                      | Cell |
 * |--------------------------------|------|
 * | offline                        | nothing asked |
 * | online, waiting for the answer | a loading marker (`hosts-row-<id>-tooling-loading`) |
 * | online, the call failed        | a said-so (`hosts-row-<id>-tooling-failed`), never a blank cell |
 * | online, answered               | `HostRowTooling` with the readings — which brings Connect |
 */

import type { JSX } from "react";

export interface HostRowToolingCellProps {
  /** The host to ask. */
  instanceId: string;
  /** Whether the registry reports this host in the live roster; gates whether it is asked at all. */
  online: boolean;
}

export function HostRowToolingCell(props: HostRowToolingCellProps): JSX.Element {
  // TODO(hosts-mount): implement
  void props;
  throw new Error("TODO(hosts-mount): implement");
}
