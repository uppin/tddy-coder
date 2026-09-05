/**
 * Watching a session connection's status from React.
 *
 * A {@link SessionConnection} publishes `status` as a value read at the moment it is asked — a
 * room's participant list changes, a bridge goes away, and nothing tells React. Sampling is
 * therefore how a component observes one, the same shape `useLiveKitPing` uses to read RTT off a
 * room's peer connection.
 *
 * TODO(optional-livekit node 6): replace the sample with a subscription once a transport that can
 * actually push a status change exists — the IPC provider has one, LiveKit's is the room's own
 * `ConnectionStateChanged`, and neither is reachable through the wire-neutral interface today.
 */

import { useEffect, useState } from "react";
import type { SessionConnection } from "./session";
import type { ConnectionStatus } from "./types";

/** How often a connection is re-read. Short enough that a handshake overlay clears promptly. */
const SAMPLE_INTERVAL_MS = 200;

export interface ObservedConnectionStatus {
  readonly status: ConnectionStatus;
  /** Why the connection is unusable, when {@link status} is `"error"`; `null` otherwise. */
  readonly error: string | null;
}

const NOT_CONNECTED: ObservedConnectionStatus = { status: "idle", error: null };

function readStatus(connection: SessionConnection | null): ObservedConnectionStatus {
  if (!connection) return NOT_CONNECTED;
  return { status: connection.status, error: connection.error };
}

/**
 * `connection`'s status, kept current for as long as the caller is mounted.
 *
 * `null` reads as `idle`: nothing has been asked of a connection that does not exist, which is a
 * different claim from one that failed.
 */
export function useConnectionStatus(
  connection: SessionConnection | null,
): ObservedConnectionStatus {
  const [observed, setObserved] = useState<ObservedConnectionStatus>(() => readStatus(connection));

  useEffect(() => {
    const sample = () =>
      setObserved((current) => {
        const next = readStatus(connection);
        // Same value, same object: a new one every 200ms would re-render every attached runtime.
        return next.status === current.status && next.error === current.error ? current : next;
      });
    sample();
    const timer = setInterval(sample, SAMPLE_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [connection]);

  return observed;
}
