import type { DaemonHost } from "tddy-client-web";

export interface CreateSessionHostFieldProps {
  daemons: DaemonHost[];
  daemonInstanceId: string;
  setDaemonInstanceId: (daemonInstanceId: string) => void;
}

/**
 * **Host** — which daemon runs the session, test id `create-session-host-select`; shown only when
 * the common room advertises daemons. Split out of `CreateSessionHostAndProjectFields` so the
 * orchestrator composer (`#orchestrators` 8/11) offers the same picker without the project select.
 * Presentational: the value stays with the caller.
 */
export function CreateSessionHostField(props: CreateSessionHostFieldProps) {
  // TODO(orchestrator-composer): implement — move the host select out of
  // `CreateSessionHostAndProjectFields` and render it from both places.
  void props;
  return null;
}
