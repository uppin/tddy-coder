/**
 * Asks one host for its tooling: one `GetHostTooling` per visit, addressed to the host through
 * `useHostClient(HostService, instanceId)`. An offline host resolves no client and is asked nothing.
 */

import { useEffect, useState } from "react";
import { HostService, type GetHostToolingResponse } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostClient } from "../../rpc/connections/registry";

/** What the one call has come to: not yet answered, answered, or failed. */
export type HostToolingReading =
  | { state: "loading" }
  | { state: "answered"; tooling: GetHostToolingResponse }
  | { state: "failed" };

/**
 * The reading of `instanceId`'s tooling. A host with no client to ask through — offline, or not
 * resolvable — reads as `failed`; callers that do not render offline hosts gate on `online`
 * themselves.
 */
export function useHostTooling(instanceId: string, online: boolean): HostToolingReading {
  // Hooks cannot be conditional, so "do not ask" is said by naming no host: an offline row resolves
  // no client and nothing is called.
  const client = useHostClient(HostService, online ? instanceId : null);
  const { sessionToken } = useAuthContext();
  const [reading, setReading] = useState<HostToolingReading>({ state: "loading" });

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
        console.debug("[useHostTooling] host did not report its tooling", instanceId, error);
        if (current) setReading({ state: "failed" });
      });
    return () => {
      current = false;
    };
  }, [client, instanceId, sessionToken]);

  return client ? reading : { state: "failed" };
}
