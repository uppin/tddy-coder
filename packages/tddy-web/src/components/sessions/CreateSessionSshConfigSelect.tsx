/**
 * Create-session SSH Host alias picker — co-located sessions, Host A.
 *
 * Empty value is LocalShell (this host). A listed alias is RemoteShell on that OpenSSH Host.
 * The list comes from `ListSshConfigHosts` on the session host (n4 retargets this to the
 * codebase host).
 */

import { useEffect, useState } from "react";
import {
  HostService,
  ProbeOutcome,
  type ListSshConfigHostsResponse,
} from "../../gen/host_pb";
import { useHostClient } from "../../rpc/connections/registry";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionSshConfigSelectProps {
  daemonInstanceId: string;
  sessionToken: string;
  value: string;
  onChange: (alias: string) => void;
}

function isOkListing(listing: ListSshConfigHostsResponse | undefined): boolean {
  return listing?.outcome === ProbeOutcome.OK;
}

function failureCaption(listing: ListSshConfigHostsResponse | undefined): string | null {
  if (listing === undefined) {
    return null;
  }
  switch (listing.outcome) {
    case ProbeOutcome.OK:
    case ProbeOutcome.UNSPECIFIED:
      return null;
    case ProbeOutcome.UNSUPPORTED:
      return "Not supported on this host";
    default:
      return listing.failureReason || "Could not list SSH hosts";
  }
}

export function CreateSessionSshConfigSelect({
  daemonInstanceId,
  sessionToken,
  value,
  onChange,
}: CreateSessionSshConfigSelectProps) {
  const client = useHostClient(HostService, daemonInstanceId);
  const [listing, setListing] = useState<ListSshConfigHostsResponse | undefined>(undefined);

  useEffect(() => {
    if (!client || !daemonInstanceId) return;
    let current = true;
    void client
      .listSshConfigHosts({ sessionToken, daemonInstanceId })
      .then((response) => {
        if (current) setListing(response);
      })
      .catch(() => {
        if (current) setListing(undefined);
      });
    return () => {
      current = false;
    };
  }, [client, daemonInstanceId, sessionToken]);

  const failure = failureCaption(listing);
  const aliases = isOkListing(listing) ? listing!.hosts.map((h) => h.alias) : [];

  return (
    <div>
      <label className={labelClass} htmlFor="create-session-ssh-config">
        SSH host
      </label>
      {failure !== null ? (
        <p className="text-xs text-muted-foreground">{failure}</p>
      ) : null}
      <select
        id="create-session-ssh-config"
        data-testid="create-session-ssh-config-select"
        className={inputClass}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={failure !== null}
      >
        <option value="">This host</option>
        {aliases.map((alias) => (
          <option key={alias} value={alias}>
            {alias}
          </option>
        ))}
      </select>
    </div>
  );
}
