import { useState } from "react";

/**
 * Which daemon/host runs the session, and the daemon its session is started on: the Host field
 * holds the chosen host's own id, which may be a derived host's, and every daemon-addressed read
 * uses the parent.
 */
export function useDerivedHostSelection(
  daemons: ReadonlyArray<{ instanceId: string; parent?: string }>,
  initialDaemonInstanceId: string,
): {
  daemonInstanceId: string;
  setDaemonInstanceId: (id: string) => void;
  sessionDaemonInstanceId: string;
} {
  // Which daemon/host runs the session. Defaults to the pre-filled host, else the selected daemon,
  // else empty (which the daemon treats as "run locally on the connected daemon"). An empty
  // pre-filled host falls through to the selected daemon so the Host <select>'s displayed option
  // matches the value it will submit.
  const [daemonInstanceId, setDaemonInstanceId] = useState(initialDaemonInstanceId);
  // The Host field holds the chosen host's own id, which may be a derived host's. Its session is
  // started on the parent daemon, so every daemon-addressed read below uses the parent.
  const selectedHostEntry = daemons.find((d) => d.instanceId === daemonInstanceId) ?? null;
  const sessionDaemonInstanceId = selectedHostEntry?.parent ?? daemonInstanceId;
  return { daemonInstanceId, setDaemonInstanceId, sessionDaemonInstanceId };
}
