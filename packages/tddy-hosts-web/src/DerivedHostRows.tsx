/**
 * The hosts one daemon reaches through a remote shell, as rows beneath that daemon's own row.
 *
 * A derived host is not a daemon: it has no telemetry, no tooling probe, no registry entry and no
 * last-seen time, so its row carries its label, its kind (`SSH`) and its id, and leaves every
 * daemon-only cell empty rather than rendering a reading nobody took.
 *
 * Listed live, per parent, with `HostService.ListDerivedHosts` over the parent's own connection —
 * and only while the parent is online, since a derived host is reachable through its parent and
 * nothing else.
 *
 * Honesty, as `HostRowSshConfig`: a listing that failed is a row of its own under the parent,
 * saying why (`hosts-row-<parent>-derived-hosts-failure`). It is never rendered as a parent that
 * reaches no hosts.
 *
 * Renders `<tr>` elements, so it is mounted inside the Hosts table's `<tbody>`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md
 */

import { safeTestIdPart } from "tddy-client-web";

export interface DerivedHostRowsProps {
  /** The daemon whose derived hosts these are — the row they are listed beneath. */
  parentInstanceId: string;
  /** Whether that daemon is in the live roster. An offline parent is not asked. */
  parentOnline: boolean;
}

/** The test id of the row of the derived host `derivedHostId` (`<parent>/ssh/<alias>`). */
export function derivedHostRowTestId(derivedHostId: string): string {
  return `hosts-derived-row-${safeTestIdPart(derivedHostId)}`;
}

/** The test id of the row saying `parentInstanceId` could not list the hosts it reaches. */
export function derivedHostsFailureTestId(parentInstanceId: string): string {
  return `hosts-row-${parentInstanceId}-derived-hosts-failure`;
}

export function DerivedHostRows({ parentInstanceId, parentOnline }: DerivedHostRowsProps) {
  // TODO(derived-hosts): implement
  void parentInstanceId;
  void parentOnline;
  return null;
}
