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

import { useEffect, useState } from "react";
import { safeTestIdPart, useAuth, useHostClient } from "tddy-client-web";
import {
  DerivedHostKind,
  HostService,
  ProbeOutcome,
  type DerivedHost,
} from "tddy-client-web/gen/host_pb";

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

function kindLabel(kind: DerivedHostKind): string {
  return kind === DerivedHostKind.SSH ? "SSH" : "";
}

export function DerivedHostRows({ parentInstanceId, parentOnline }: DerivedHostRowsProps) {
  const client = useHostClient(HostService, parentInstanceId);
  const { sessionToken } = useAuth();
  const [hosts, setHosts] = useState<DerivedHost[]>([]);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    if (!parentOnline || !client) return;
    let current = true;
    client
      .listDerivedHosts({ sessionToken: sessionToken ?? "", daemonInstanceId: parentInstanceId })
      .then((response) => {
        if (!current) return;
        setHosts([...response.hosts]);
        setFailure(
          response.outcome === ProbeOutcome.FAILED
            ? response.failureReason || "Listing derived hosts failed"
            : null,
        );
      })
      .catch((error: unknown) => {
        if (!current) return;
        setHosts([]);
        setFailure(error instanceof Error ? error.message : "Listing derived hosts failed");
      });
    return () => {
      current = false;
    };
  }, [client, parentInstanceId, parentOnline, sessionToken]);

  if (!parentOnline) return null;

  return (
    <>
      {hosts.map((host) => {
        const rowId = derivedHostRowTestId(host.id);
        return (
          <tr key={host.id} data-testid={rowId}>
            <td className="py-2 pr-4">{host.label}</td>
            <td className="py-2 pr-4">
              <span data-testid={`${rowId}-kind`}>{kindLabel(host.kind)}</span>
            </td>
            <td className="py-2 pr-4" />
            <td className="py-2 pr-4" />
            <td className="py-2 pr-4" />
            <td className="py-2 pr-4 font-mono text-xs">{host.id}</td>
            <td className="py-2" />
          </tr>
        );
      })}
      {failure !== null ? (
        <tr data-testid={derivedHostsFailureTestId(parentInstanceId)}>
          <td className="py-2 pr-4 text-sm text-destructive" colSpan={7}>
            Could not list the hosts this daemon reaches: {failure}
          </td>
        </tr>
      ) : null}
    </>
  );
}
