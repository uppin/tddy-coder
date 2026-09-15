/**
 * SSH connections cell: the explicit OpenSSH `Host` aliases on this host.
 *
 * Three states, kept distinct because each is a different next action:
 *
 * | State | What an operator does |
 * |---|---|
 * | aliases listed | pick one later as a session destination |
 * | none | LocalShell is the only choice; this host has no Host aliases |
 * | could not check | do not treat this as "no destinations" |
 *
 * Honesty: a failed read is never rendered as an empty list.
 */

import { useEffect, useState } from "react";
import { HostService, ProbeOutcome, type ListSshConfigHostsResponse } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostClient } from "../../rpc/connections/registry";

export interface HostRowSshConfigProps {
  instanceId: string;
}

/** What the section says when it is not listing aliases, and what hovering it explains. */
interface ConfigSummary {
  text: string;
  title: string;
}

function awaitingListing(): ConfigSummary {
  return { text: "…", title: "Waiting for SSH config hosts from this host" };
}

function summaryOf(listing: ListSshConfigHostsResponse | undefined): ConfigSummary | null {
  if (listing === undefined) {
    return awaitingListing();
  }
  switch (listing.outcome) {
    case ProbeOutcome.OK:
      break;
    case ProbeOutcome.UNSPECIFIED:
      return awaitingListing();
    case ProbeOutcome.UNSUPPORTED:
      return {
        text: "Not supported here",
        title: "This host cannot list SSH config Host aliases",
      };
    default:
      return {
        text: "Could not check",
        title: listing.failureReason
          ? `SSH config could not be read: ${listing.failureReason}`
          : "SSH config could not be read, so no destinations are known",
      };
  }
  if (listing.hosts.length === 0) {
    return {
      text: "No SSH hosts",
      title: "This host's SSH config has no explicit Host aliases",
    };
  }
  return null;
}

function ListedAlias({ instanceId, alias }: { instanceId: string; alias: string }) {
  return (
    <span
      data-testid={`hosts-row-${instanceId}-ssh-config-host-${alias}`}
      className="flex items-baseline gap-1"
      title={`An explicit Host alias in this host's ~/.ssh/config`}
    >
      {alias}
    </span>
  );
}

export function HostRowSshConfig({ instanceId }: HostRowSshConfigProps) {
  const client = useHostClient(HostService, instanceId);
  const { sessionToken } = useAuthContext();
  const [listing, setListing] = useState<ListSshConfigHostsResponse | undefined>(undefined);

  useEffect(() => {
    if (!client) return;
    let current = true;
    void client
      .listSshConfigHosts({ sessionToken: sessionToken ?? "", daemonInstanceId: instanceId })
      .then((response) => {
        if (current) setListing(response);
      })
      .catch((error: unknown) => {
        console.debug("[HostRowSshConfig] host offered no SSH config listing", instanceId, error);
      });
    return () => {
      current = false;
    };
  }, [client, instanceId, sessionToken]);

  const summary = summaryOf(listing);

  return (
    <span
      data-testid={`hosts-row-${instanceId}-ssh-config`}
      className="flex items-baseline gap-1 text-xs text-muted-foreground"
    >
      <span className="mr-1 opacity-70">ssh</span>
      {summary === null ? (
        <span className="flex flex-col gap-1">
          {(listing?.hosts ?? []).map((host) => (
            <ListedAlias key={host.alias} instanceId={instanceId} alias={host.alias} />
          ))}
        </span>
      ) : (
        <span title={summary.title}>{summary.text}</span>
      )}
    </span>
  );
}
