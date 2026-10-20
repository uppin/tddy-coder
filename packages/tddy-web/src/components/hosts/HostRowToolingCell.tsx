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

import { useEffect, useState, type JSX } from "react";
import { HostService, type GetHostToolingResponse } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostClient } from "../../rpc/connections/registry";
import { HostRowTooling } from "./HostRowTooling";

export interface HostRowToolingCellProps {
  /** The host to ask. */
  instanceId: string;
  /** Whether the registry reports this host in the live roster; gates whether it is asked at all. */
  online: boolean;
}

/** What the one call has come to: not yet answered, answered, or failed. */
type Reading =
  | { state: "loading" }
  | { state: "answered"; tooling: GetHostToolingResponse }
  | { state: "failed" };

export function HostRowToolingCell({ instanceId, online }: HostRowToolingCellProps): JSX.Element | null {
  // Hooks cannot be conditional, so "do not ask" is said by naming no host: an offline row resolves
  // no client and nothing is called.
  const client = useHostClient(HostService, online ? instanceId : null);
  const { sessionToken } = useAuthContext();
  const [reading, setReading] = useState<Reading>({ state: "loading" });

  useEffect(() => {
    if (!client) return;
    let current = true;
    setReading({ state: "loading" });
    client
      .getHostTooling({ sessionToken: sessionToken ?? "", daemonInstanceId: instanceId })
      .then((tooling) => {
        if (current) setReading({ state: "answered", tooling });
      })
      .catch((error: unknown) => {
        console.debug("[HostRowToolingCell] host did not report its tooling", instanceId, error);
        if (current) setReading({ state: "failed" });
      });
    return () => {
      current = false;
    };
  }, [client, instanceId, sessionToken]);

  if (!online) return null;

  // An online row with no client has no wire to ask through: said so, not left blank.
  if (!client || reading.state === "failed") {
    return (
      <span
        data-testid={`hosts-row-${instanceId}-tooling-failed`}
        className="text-xs text-muted-foreground"
        title="The host did not answer its tooling request"
      >
        Tooling could not be read
      </span>
    );
  }

  if (reading.state === "loading") {
    return (
      <span
        data-testid={`hosts-row-${instanceId}-tooling-loading`}
        className="text-xs text-muted-foreground"
      >
        …
      </span>
    );
  }

  return (
    <HostRowTooling
      instanceId={instanceId}
      git={reading.tooling.git}
      githubCli={reading.tooling.githubCli}
      sshAgent={reading.tooling.sshAgent}
      remoteDesktop={reading.tooling.remoteDesktop}
    />
  );
}
